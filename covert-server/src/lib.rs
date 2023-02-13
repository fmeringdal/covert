#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![deny(clippy::pedantic)]
#![deny(clippy::get_unwrap)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]

mod core;
mod error;
mod expiration_manager;
mod helpers;
mod layer;
mod response;
mod router;
mod store;
mod system;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use covert_framework::extract::Extension;
use covert_storage::EncryptedPool;
use covert_types::state::VaultState;
pub use expiration_manager::{ExpirationManager, LeaseEntry};
pub use router::{Router, RouterService};
use tower::{make::Shared, ServiceBuilder};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};
use tracing::info;
use tracing_error::ErrorLayer;
use tracing_subscriber::{prelude::*, EnvFilter};

use crate::{
    expiration_manager::clock::SystemClock,
    layer::{
        auth_service::AuthServiceLayer, core_extension::CoreStateInjectorLayer,
        lease_registration::LeaseRegistrationLayer, request_mapper::LogicalRequestResponseLayer,
    },
    store::{
        identity_store::IdentityStore, lease_store::LeaseStore, mount_store::MountStore,
        policy_store::PolicyStore, token_store::TokenStore,
    },
};

pub use self::core::Core;

pub struct Config {
    pub storage_path: String,
    pub port: u16,
}

async fn shutdown_signal(core: Arc<Core>) {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
    info!("Shutdown signal received");
    if core.state() == VaultState::Unsealed && core.seal().await.is_err() {
        tracing::error!("Failed to seal Vault");
    }
}

pub async fn start(config: Config) -> Result<(), anyhow::Error> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("hyper=off,debug"));

    let subscriber = tracing_subscriber::Registry::default()
        .with(ErrorLayer::default())
        .with(env_filter)
        .with(tracing_subscriber::fmt::Layer::default());

    // set the subscriber as the default for the application
    tracing::subscriber::set_global_default(subscriber)
        .expect("failed to setup tracing subscriber");

    let router = Arc::new(Router::new());
    let encrypted_pool = Arc::new(EncryptedPool::new(&config.storage_path));

    let mount_store = Arc::new(MountStore::new(encrypted_pool.clone()));
    let lease_store = Arc::new(LeaseStore::new(encrypted_pool.clone()));
    let token_store = Arc::new(TokenStore::new(encrypted_pool.clone()));
    let policy_store = Arc::new(PolicyStore::new(encrypted_pool.clone()));
    let identity_store = Arc::new(IdentityStore::new(encrypted_pool.clone()));

    let expiration = Arc::new(ExpirationManager::new(
        Arc::clone(&router),
        lease_store,
        mount_store.clone(),
        SystemClock::new(),
    ));

    let core = Arc::new(Core::new(
        encrypted_pool.clone(),
        router.clone(),
        expiration.clone(),
        identity_store.clone(),
        policy_store.clone(),
        token_store.clone(),
        mount_store,
    ));

    core.mount_internal_backends().await?;

    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
    info!("listening on {addr}");

    let server_router_svc = ServiceBuilder::new()
        .concurrency_limit(1000)
        .timeout(Duration::from_secs(30))
        .layer(RequestBodyLimitLayer::new(1024 * 16))
        .layer(CorsLayer::permissive())
        .layer(LogicalRequestResponseLayer::new())
        .layer(CoreStateInjectorLayer::new(core.clone()))
        .layer(AuthServiceLayer::new(token_store.clone()))
        .layer(LeaseRegistrationLayer::new(
            expiration.clone(),
            token_store.clone(),
            identity_store.clone(),
        ))
        .layer(Extension(core.clone()))
        .service(RouterService::new(router.clone()));

    let vault_server = hyper::Server::bind(&addr)
        .serve(Shared::new(server_router_svc))
        .with_graceful_shutdown(shutdown_signal(core));

    // And run forever...
    if let Err(error) = vault_server.await {
        tracing::error!(?error, "Encountered server error. Shutting down.");
        return Err(error.into());
    }
    Ok(())
}
