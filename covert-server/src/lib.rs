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
use covert_storage::EncryptedPool;
use covert_types::state::StorageState;
pub use expiration_manager::{ExpirationManager, LeaseEntry};
pub use router::{Router, RouterService};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use tokio::sync::broadcast;
use tower::{make::Shared, ServiceBuilder};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};
use tracing::{info, warn};

use crate::{
    context::Context,
    expiration_manager::clock::SystemClock,
    layer::{
        auth_service::AuthServiceLayer, lease_registration::LeaseRegistrationLayer,
        namespace_extension::NamespaceExtensionLayer, request_mapper::LogicalRequestResponseLayer,
        storage_state_extension::StorageStateExtensionLayer,
    },
    recovery::{has_encrypted_storage_backup, recover, replicate},
    repos::Repos,
    system::new_system_backend,
};

pub async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

pub async fn start(
    mut config: Config,
    shutdown_signal: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    info!("Starting covert");
    config.sanitize()?;

    // TODO
    // let child_processes = ChildProcesses::default();
    let (stop_tx, _stop_rx) = broadcast::channel(1);
    let stop_tx_cloned = stop_tx.clone();
    let shutdown_handler = async move {
        shutdown_signal.await;
        info!("Shutdown signal received");
        // TODO:
        stop_tx_cloned.send(()).unwrap();
    };

    let port_tx = config.port_tx.take();
    let config = Arc::new(config);

    // Try to recover as far as possible if replication is configured and we
    // have a backup available
    let mut initial_storage_state = StorageState::Uninitialized;
    if let Some(replication) = config.replication.as_ref() {
        std::env::set_var("AWS_ACCESS_KEY_ID", &replication.access_key_id);
        std::env::set_var("AWS_SECRET_ACCESS_KEY", &replication.secret_access_key);
        // Recover seal storage
        info!("Recover seal storage");
        recover(
            replication,
            &config.seal_storage_path(),
            &replication.seal_db_prefix(),
            None,
        )
        .await?;
        info!("Recover seal storage done");

        // Recover latest snapshot of encrypted storage. Changes applied to DB
        // after latest snapshot will be applied after we have the encryption key
        if has_encrypted_storage_backup(replication).await? {
            initial_storage_state = StorageState::Sealed;
        }
        // available during unseal.
        // info!("Recover encryted storage");
        // recover_encrypted_storage_snapshot(&config, replication).await;
        // info!("Recover encryted storage done");
    } else {
        warn!("No replication enabled");
    }

    // Create seal storage DB
    if tokio::fs::try_exists(config.seal_storage_path()).await? {
        initial_storage_state = StorageState::Sealed;
    }
    let seal_db = sqlx::sqlite::SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .foreign_keys(true)
                .synchronous(SqliteSynchronous::Full)
                .pragma("wal_autocheckpoint", "0")
                .pragma("busy_timeout", "5000")
                .filename(config.seal_storage_path()),
        )
        .await
        .unwrap();

    // Start replication of seal storage if configured
    if let Some(replication) = config.replication.as_ref() {
        replicate(
            replication,
            None,
            &config.seal_storage_path(),
            &replication.seal_db_prefix(),
            stop_tx.subscribe(),
        )
        .await
        .unwrap();
    }

    let encrypted_pool = Arc::new(EncryptedPool::new(
        &config.encrypted_storage_path(),
        initial_storage_state,
    ));
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
        expiration_manager: Arc::clone(&expiration),
        router: Arc::clone(&router),
        stop_tx,
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

    repos.close().await;

    info!("Covert server shut down");
    Ok(())
}
