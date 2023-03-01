#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![deny(clippy::pedantic)]
#![deny(clippy::get_unwrap)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]

mod config;
mod context;
mod error;
mod expiration_manager;
mod helpers;
mod layer;
mod migrations;
mod recovery;
mod repos;
mod response;
mod router;
mod system;

use std::{future::Future, net::SocketAddr, sync::Arc, time::Duration};

pub use config::*;
use context::ChildProcesses;
use covert_storage::EncryptedPool;
pub use expiration_manager::{ExpirationManager, LeaseEntry};
pub use router::{Router, RouterService};
use sqlx::sqlite::SqliteConnectOptions;
use tower::{make::Shared, ServiceBuilder};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};
use tracing::info;

use crate::{
    context::Context,
    expiration_manager::clock::SystemClock,
    layer::{
        auth_service::AuthServiceLayer, lease_registration::LeaseRegistrationLayer,
        namespace_extension::NamespaceExtensionLayer, request_mapper::LogicalRequestResponseLayer,
        storage_state_extension::StorageStateExtensionLayer,
    },
    recovery::{recover, recover_encrypted_storage_snapshot, replicate},
    repos::Repos,
    system::new_system_backend,
};

pub async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

async fn shutdown_handler(child_processes: ChildProcesses) {
    child_processes.kill_all().await;
}

pub async fn start(
    mut config: Config,
    shutdown_signal: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    config.sanitize()?;

    let child_processes = ChildProcesses::default();
    let shutdown_handler = async {
        shutdown_signal.await;
        info!("Shutdown signal received");
        shutdown_handler(child_processes.clone()).await;
    };

    let port_tx = config.port_tx.take();
    let config = Arc::new(config);

    // Try to recover as far as possible if replication has configured and we
    // have a backup available
    if let Some(replication) = config.replication.as_ref() {
        // Recover seal storage
        recover(
            replication,
            &config.seal_storage_path(),
            &replication.seal_bucket_url(),
        )?;

        // Recover latest snapshot of encrypted storage. Changes applied to DB
        // after latest snapshot will be applied after we have the encryption key
        // available during unseal.
        recover_encrypted_storage_snapshot(&config, replication);
    }

    // Create seal storage DB
    let seal_db = sqlx::sqlite::SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .create_if_missing(true)
                .foreign_keys(true)
                .filename(config.seal_storage_path()),
        )
        .await?;

    // Start replication of seal storage if configured
    if let Some(replication) = config.replication.as_ref() {
        let p = replicate(
            replication,
            None,
            &config.seal_storage_path(),
            &replication.seal_bucket_url(),
        )?;
        child_processes.set_seal_storage_replication(p).await;
    }

    let encrypted_pool = Arc::new(EncryptedPool::new(&config.encrypted_storage_path()));
    let repos = Repos::new(encrypted_pool, seal_db);

    // Run migration
    crate::migrations::migrate_unecrypted_db(&repos.unecrypted_pool).await?;

    let router = Arc::new(Router::new(repos.mount.clone()));
    let expiration = Arc::new(ExpirationManager::new(
        Arc::clone(&router),
        repos.clone(),
        SystemClock::new(),
    ));
    let ctx = Context {
        config: Arc::clone(&config),
        repos: repos.clone(),
        child_processes: child_processes.clone(),
        expiration_manager: Arc::clone(&expiration),
        router: Arc::clone(&router),
    };

    // Mount system backend
    let system = new_system_backend(ctx);
    router.mount_system(Arc::new(system));

    let server_router_svc = ServiceBuilder::new()
        .concurrency_limit(1000)
        .timeout(Duration::from_secs(30))
        .layer(RequestBodyLimitLayer::new(1024 * 16))
        .layer(CorsLayer::permissive())
        .layer(LogicalRequestResponseLayer::new())
        .layer(StorageStateExtensionLayer::new(Arc::clone(&repos.pool)))
        .layer(NamespaceExtensionLayer::new(repos.namespace.clone()))
        .layer(AuthServiceLayer::new(
            repos.token.clone(),
            repos.namespace.clone(),
        ))
        .layer(LeaseRegistrationLayer::new(
            expiration.clone(),
            repos.token.clone(),
            repos.entity.clone(),
        ))
        .service(RouterService::new(router.clone()));

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let covert_server = hyper::Server::bind(&addr).serve(Shared::new(server_router_svc));
    let addr = covert_server.local_addr();
    let covert_server = covert_server.with_graceful_shutdown(shutdown_handler);

    info!("listening on {addr}");
    if let Some(tx) = port_tx {
        let _ = tx.send(addr.port());
    }

    // And run forever...
    if let Err(error) = covert_server.await {
        tracing::error!(?error, "Encountered server error. Shutting down.");
        return Err(error.into());
    }
    Ok(())
}
