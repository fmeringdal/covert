#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![deny(clippy::pedantic)]
#![deny(clippy::get_unwrap)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]

mod error;
mod expiration_manager;
mod helpers;
mod layer;
mod migrations;
mod repos;
mod response;
mod router;
mod system;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use covert_storage::EncryptedPool;
use covert_types::{backend::BackendType, mount::MountConfig};
pub use expiration_manager::{ExpirationManager, LeaseEntry};
pub use router::{Router, RouterService};
use sqlx::sqlite::SqliteConnectOptions;
use tokio::sync::oneshot;
use tower::{make::Shared, ServiceBuilder};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};
use tracing::info;

use crate::{
    expiration_manager::clock::SystemClock,
    layer::{
        auth_service::AuthServiceLayer, core_extension::CoreStateInjectorLayer,
        lease_registration::LeaseRegistrationLayer, request_mapper::LogicalRequestResponseLayer,
    },
    repos::Repos,
    system::SYSTEM_MOUNT_PATH,
};

pub struct Config {
    pub storage_path: String,
    pub seal_storage_path: String,
    pub port: u16,
    pub port_tx: Option<oneshot::Sender<u16>>,
}

async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
    info!("Shutdown signal received");
}

pub async fn start(config: Config) -> Result<(), anyhow::Error> {
    let router = Arc::new(Router::new());
    let encrypted_pool = Arc::new(EncryptedPool::new(&config.storage_path));

    let connect_opts = SqliteConnectOptions::new()
        .create_if_missing(true)
        .foreign_keys(true)
        .filename(&config.seal_storage_path);

    let unecrypted_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .connect_with(connect_opts)
        .await?;
    let repos = Repos::new(encrypted_pool, unecrypted_pool);

    // Run migration
    crate::migrations::migrate_unecrypted_db(&repos.unecrypted_pool).await?;

    let expiration = Arc::new(ExpirationManager::new(
        Arc::clone(&router),
        repos.lease.clone(),
        repos.mount.clone(),
        SystemClock::new(),
    ));

    // Mount system backend
    crate::system::mount(
        &repos,
        Arc::clone(&expiration),
        Arc::clone(&router),
        SYSTEM_MOUNT_PATH.to_string(),
        BackendType::System,
        MountConfig::default(),
        true,
    )
    .await?;

    let server_router_svc = ServiceBuilder::new()
        .concurrency_limit(1000)
        .timeout(Duration::from_secs(30))
        .layer(RequestBodyLimitLayer::new(1024 * 16))
        .layer(CorsLayer::permissive())
        .layer(LogicalRequestResponseLayer::new())
        .layer(CoreStateInjectorLayer::new(Arc::clone(&repos.pool)))
        .layer(AuthServiceLayer::new(repos.token.clone()))
        .layer(LeaseRegistrationLayer::new(
            expiration.clone(),
            repos.token.clone(),
            repos.entity.clone(),
        ))
        .service(RouterService::new(router.clone()));

    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
    let covert_server = hyper::Server::bind(&addr).serve(Shared::new(server_router_svc));
    let addr = covert_server.local_addr();
    let covert_server = covert_server.with_graceful_shutdown(shutdown_signal());

    info!("listening on {addr}");
    if let Some(tx) = config.port_tx {
        let _ = tx.send(addr.port());
    }

    // And run forever...
    if let Err(error) = covert_server.await {
        tracing::error!(?error, "Encountered server error. Shutting down.");
        return Err(error.into());
    }
    Ok(())
}
