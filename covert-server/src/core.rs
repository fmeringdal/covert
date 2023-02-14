use std::sync::Arc;

use crate::error::{Error, ErrorType};
use crate::router::RouteEntry;
use crate::store::identity_store::IdentityStore;
use crate::store::mount_store::MountStore;
use crate::store::policy_store::PolicyStore;
use crate::store::token_store::{TokenEntry, TokenStore};
use crate::system::new_system_backend;

use super::{expiration_manager::ExpirationManager, router::Router};
use covert_framework::Backend;
use covert_kv::new_versioned_kv_backend;
use covert_psql::new_psql_backend;
use covert_storage::migrator::MigrationError;
use covert_storage::{BackendStoragePool, EncryptedPool};
use covert_types::backend::BackendType;
use covert_types::entity::Entity;
use covert_types::mount::{MountConfig, MountEntry};
use covert_types::policy::Policy;
use covert_types::state::VaultState;
use covert_types::token::Token;
use covert_userpass_auth::new_userpass_backend;
use rust_embed::RustEmbed;
use tracing::info;
use uuid::Uuid;

pub const SYSTEM_MOUNT_PATH: &str = "sys/";

#[derive(RustEmbed)]
#[folder = "migrations/"]
struct Migrations;

pub struct Core {
    encrypted_pool: Arc<EncryptedPool>,
    router: Arc<Router>,
    expiration_manager: Arc<ExpirationManager>,
    identity_store: Arc<IdentityStore>,
    policy_store: Arc<PolicyStore>,
    token_store: Arc<TokenStore>,
    mounts_store: Arc<MountStore>,
}

impl Core {
    #[must_use]
    pub fn new(
        encrypted_pool: Arc<EncryptedPool>,
        router: Arc<Router>,
        expiration_manager: Arc<ExpirationManager>,
        identity_store: Arc<IdentityStore>,
        policy_store: Arc<PolicyStore>,
        token_store: Arc<TokenStore>,
        mounts_store: Arc<MountStore>,
    ) -> Self {
        Self {
            encrypted_pool,
            router,
            expiration_manager,
            identity_store,
            policy_store,
            token_store,
            mounts_store,
        }
    }

    pub async fn generate_root_token(&self) -> Result<Token, Error> {
        // Generate root policy if not exist
        let policy = Policy::new("root".into(), vec![]);
        let _res = self.policy_store.create(&policy).await;

        // Generate root entity if not exist
        let entity = Entity::new("root".into(), false);
        let _res = self.identity_store.create(&entity).await;

        let te = TokenEntry::new_root();
        let token = te.id().clone();
        self.token_store.create(&te).await?;

        Ok(token)
    }

    #[must_use]
    pub fn state(&self) -> VaultState {
        self.encrypted_pool.state()
    }

    pub fn initialize(&self) -> Result<Option<String>, Error> {
        info!("Initializing the storage");
        self.encrypted_pool.initialize().map_err(Into::into)
    }

    // TODO: this should be improved
    pub(crate) async fn migrate<M: rust_embed::RustEmbed>(
        pool: &EncryptedPool,
    ) -> Result<(), Error> {
        let migrations = covert_storage::migrator::migration_scripts::<M>()?;

        for migration in migrations {
            sqlx::query(&migration.script)
                .execute(pool)
                .await
                .map_err(|error| MigrationError::Execution {
                    filename: migration.description,
                    error,
                })?;
        }
        Ok(())
    }

    pub async fn unseal(&self, master_key: String) -> Result<(), Error> {
        self.encrypted_pool.unseal(master_key)?;

        // Run migrations
        Self::migrate::<Migrations>(&self.encrypted_pool).await?;

        let mounts = self.mounts_store.list().await?;
        if mounts.is_empty() {
            // TODO: Insert default KV backend
        }

        for mount in mounts {
            self.mount_route_entry(mount.path, mount.uuid, mount.backend_type, mount.config)
                .await?;
        }

        // Start expiration manager
        let expiration_manager = self.expiration_manager.clone();
        tokio::spawn(async move {
            if expiration_manager.start().await.is_err() {
                // TODO: stop the server
            }
        });

        Ok(())
    }

    pub async fn seal(&self) -> Result<(), Error> {
        info!("Sealing the core");
        self.encrypted_pool.seal()?;

        // Stop expiration manager
        self.expiration_manager.stop().await;

        // Clear all the route entries except system
        self.router.clear_mounts().await;
        self.mount_internal_backends().await?;

        Ok(())
    }

    pub async fn mount_internal_backends(&self) -> Result<(), Error> {
        let config = MountConfig::default();

        // System
        self.mount(
            SYSTEM_MOUNT_PATH.to_string(),
            BackendType::System,
            config.clone(),
            true,
        )
        .await?;

        Ok(())
    }

    #[must_use]
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Mount a new backend
    pub async fn mount(
        &self,
        path: String,
        variant: BackendType,
        config: MountConfig,
        internal: bool,
    ) -> Result<Uuid, Error> {
        // Mount internally
        let uuid = Uuid::new_v4();
        let (backend, prefix) = self
            .mount_route_entry(path.clone(), uuid, variant, config.clone())
            .await?;

        let is_internal_backend = matches!(variant, BackendType::System);

        if is_internal_backend && !internal {
            return Err(ErrorType::InvalidMountType { variant }.into());
        }

        if !is_internal_backend {
            let entry = MountEntry {
                uuid,
                path,
                config,
                backend_type: variant,
            };
            // TODO: remove entry from the internal router if it fails to store in db
            self.mounts_store.create(&entry).await?;
        }
        if !backend.migrations.is_empty() {
            backend
                .migrate(Arc::clone(&self.encrypted_pool), &uuid.to_string(), &prefix)
                .await
                .map_err(|error| ErrorType::BackendMigration { error, variant })?;
        }

        Ok(uuid)
    }

    async fn mount_route_entry(
        &self,
        path: String,
        uuid: Uuid,
        variant: BackendType,
        config: MountConfig,
    ) -> Result<(Arc<Backend>, String), Error> {
        let backend_storage = self.storeage_pool_for_backend(&uuid, variant);

        let prefix = backend_storage.prefix().to_string();
        let backend = Arc::new(self.new_backend(variant, backend_storage).await?);

        let re = RouteEntry::new(uuid, path, Arc::clone(&backend), config)?;
        self.router.mount(re).await?;

        Ok((backend, prefix))
    }

    fn storeage_pool_for_backend(&self, id: &Uuid, variant: BackendType) -> BackendStoragePool {
        BackendStoragePool::new(
            &variant.to_string(),
            &id.to_string().replace('-', ""),
            Arc::clone(&self.encrypted_pool),
        )
    }

    pub async fn update_mount(&self, path: &str, config: MountConfig) -> Result<MountEntry, Error> {
        let mut me = self
            .mounts_store
            .get_by_path(path)
            .await?
            .ok_or_else(|| ErrorType::MountNotFound { path: path.into() })?;
        me.config = config;
        self.mounts_store.set_config(me.uuid, &me.config).await?;

        Ok(me)
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_mount(&self, path: &str) -> Result<MountEntry, Error> {
        // Check that it is not system
        // Revoke all leases
        // Remove from router
        // Remove mount from mount store
        // TODO: Remove all the tables for that mount
        let me = self
            .mounts_store
            .get_by_path(path)
            .await?
            .ok_or_else(|| ErrorType::MountNotFound { path: path.into() })?;
        if me.backend_type == BackendType::System {
            return Err(ErrorType::InvalidMountType {
                variant: BackendType::System,
            }
            .into());
        }

        self.expiration_manager
            .revoke_leases_by_mount_prefix(path)
            .await?;
        if !self.router.remove(path).await {
            return Err(ErrorType::MountNotFound { path: path.into() }.into());
        }
        self.mounts_store.remove_by_path(path).await?;

        // Delete all storage for the mount
        let storage = self.storeage_pool_for_backend(&me.uuid, me.backend_type);
        let storage_prefix = storage.prefix();
        let tables = crate::helpers::sqlite::get_resources_by_prefix(
            self.encrypted_pool.as_ref(),
            storage_prefix,
        )
        .await?;

        for table in tables {
            info!("Dropping table {}", table.name);
            crate::helpers::sqlite::drop_table(self.encrypted_pool.as_ref(), &table.name).await?;
        }

        Ok(me)
    }

    async fn new_backend(
        &self,
        variant: BackendType,
        storage: BackendStoragePool,
    ) -> Result<Backend, MigrationError> {
        match variant {
            BackendType::Kv => new_versioned_kv_backend(storage),
            BackendType::Postgres => new_psql_backend(storage).await,
            BackendType::System => Ok(new_system_backend(
                self.token_store.clone(),
                self.policy_store.clone(),
                self.identity_store.clone(),
                self.expiration_manager.clone(),
            )),
            BackendType::Userpass => new_userpass_backend(storage),
        }
    }
}
