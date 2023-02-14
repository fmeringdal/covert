#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![deny(clippy::pedantic)]
#![deny(clippy::get_unwrap)]
#![allow(clippy::module_name_repetitions)]

mod error;
mod path_config_connection;
mod path_role_create;
mod path_roles;
mod secret_creds;
mod store;

use std::sync::Arc;

use covert_storage::{
    migrator::{migration_scripts, MigrationError},
    BackendStoragePool,
};
use error::{Error, ErrorType};
use rust_embed::RustEmbed;
use secret_creds::secret_creds_renew;
use sqlx::{postgres::PgPoolOptions, PgPool, Pool, Postgres};
use store::{connection::ConnectionStore, role::RoleStore};
use tokio::sync::{RwLock, RwLockReadGuard};

use covert_framework::{extract::Extension, read, revoke, update, Backend, Router};
use covert_types::{
    backend::{BackendCategory, BackendType},
    psql::ConnectionConfig,
};
use tracing::debug;

use self::{
    path_config_connection::{path_connection_read, path_connection_write},
    path_role_create::generate_role_credentials,
    path_roles::path_role_create,
    secret_creds::secret_creds_revoke,
};

#[derive(RustEmbed)]
#[folder = "migrations/"]
struct Migrations;

pub struct Context {
    db: RwLock<Option<PgPool>>,
    connection_repo: ConnectionStore,
    role_repo: RoleStore,
}

/// Returns a new `PostgreSQL` secret engine.
///
/// # Errors
///
/// Returns an error if it fails to read the migration scripts.
#[tracing::instrument(skip_all)]
pub async fn new_psql_backend(storage: BackendStoragePool) -> Result<Backend, MigrationError> {
    let ctx = Arc::new(Context {
        db: RwLock::default(),
        connection_repo: ConnectionStore::new(storage.clone()),
        role_repo: RoleStore::new(storage),
    });

    // Try to recover pool from the connection config if it is configured.
    if ctx.set_pool().await.is_ok() {
        debug!("Configured pool from previosuly stored connection configuration");
    }

    let router = Router::new()
        .route(
            "/config/connection",
            read(path_connection_read)
                .update(path_connection_write)
                .create(path_connection_write),
        )
        .route("/creds/:name", update(generate_role_credentials))
        .route(
            "/roles/:name",
            update(path_role_create).create(path_role_create),
        )
        .route(
            "/creds",
            revoke(secret_creds_revoke).renew(secret_creds_renew),
        )
        .layer(Extension(ctx))
        .build()
        .into_service();

    let migrations = migration_scripts::<Migrations>()?;

    Ok(Backend {
        handler: router,
        category: BackendCategory::Logical,
        variant: BackendType::Postgres,
        migrations,
    })
}

impl Context {
    #[tracing::instrument(skip_all)]
    async fn handle_missing_pool_for_configured_connection<'a>(
        &self,
    ) -> Result<RwLockReadGuard<'a, PgPool>, Error> {
        // Something is wrong with the pool, so close it and reset connection
        self.reset_db().await;
        self.connection_repo.remove().await?;
        Err(ErrorType::MissingConnection.into())
    }

    /// Return a psql connection pool.
    ///
    /// # Errors
    ///
    /// Fails if the pool is not yet configured.
    pub async fn pool(&self) -> Result<RwLockReadGuard<'_, PgPool>, Error> {
        let pool_l = self.db.read().await;
        match RwLockReadGuard::try_map(pool_l, |maybe_pool| match maybe_pool {
            Some(pool) => Some(pool),
            None => None,
        }) {
            Ok(res) => Ok(res),
            Err(lock) => {
                drop(lock);
                match self.connection_repo.get().await? {
                    Some(_) => self.handle_missing_pool_for_configured_connection().await,
                    None => Err(ErrorType::MissingConnection.into()),
                }
            }
        }
    }

    /// Set the psql connection pool.
    ///
    /// # Errors
    ///
    /// Fails if it fails to establish a connection to the database from the
    /// connection configuration.
    pub async fn set_pool(&self) -> Result<(), Error> {
        // Reset any existing pool
        self.reset_db().await;

        let conn_config = self
            .connection_repo
            .get()
            .await?
            .ok_or(ErrorType::MissingConnection)?;
        let pool = pool_from_config(&conn_config).await?;

        let mut pool_wl = self.db.write().await;
        *pool_wl = Some(pool);

        Ok(())
    }

    async fn reset_db(&self) {
        let mut pool = self.db.write().await;
        if let Some(pool) = pool.as_ref() {
            pool.close().await;
        }
        *pool = None;
    }
}

pub(crate) async fn pool_from_config(config: &ConnectionConfig) -> Result<Pool<Postgres>, Error> {
    let mut connection_url = config.connection_url.clone();

    // Ensure timezone is set to UTC for all the connections
    if connection_url.starts_with("postgres://") || connection_url.starts_with("postgresql://") {
        if connection_url.contains('?') {
            connection_url = format!("{connection_url}&timezone=utc");
        } else {
            connection_url = format!("{connection_url}?timezone=utc");
        }
    } else {
        connection_url = format!("{connection_url} timezone=utc");
    }

    PgPoolOptions::new()
        // Set some connection pool settings. We don't need much of this,
        // since the request rate shouldn't be high.
        .max_connections(config.max_open_connections)
        .test_before_acquire(true)
        .connect(&connection_url)
        .await
        .map_err(|_| ErrorType::InvalidConnectionString.into())
}
