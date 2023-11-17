use std::{str::FromStr, sync::Arc};

use covert_framework::{
    extract::{Extension, Json, Path},
    Backend,
};
use covert_kv::new_versioned_kv_backend;
use covert_psql::new_psql_backend;
use covert_storage::{migrator::MigrationError, BackendStoragePool, EncryptedPool};
use covert_types::{
    backend::BackendCategory,
    backend::BackendType,
    methods::system::{
        CreateMountParams, CreateMountResponse, DisableMountResponse, MountsListItemResponse,
        MountsListResponse, UpdateMountParams, UpdateMountResponse,
    },
    mount::{MountConfig, MountEntry},
    response::Response,
};
use covert_userpass_auth::new_userpass_backend;
use tracing::info;
use uuid::Uuid;

use crate::{
    context::Context,
    error::{Error, ErrorType},
    repos::{namespace::Namespace, Repos},
};

use super::{new_system_backend, SYSTEM_MOUNT_PATH};

#[tracing::instrument(skip(ctx))]
pub async fn handle_mount(
    Extension(ctx): Extension<Context>,
    Extension(ns): Extension<Namespace>,
    Path(path): Path<String>,
    Json(body): Json<CreateMountParams>,
) -> Result<Response, Error> {
    let id = mount(
        &ctx,
        path.clone(),
        ns.id.clone(),
        body.variant,
        body.config.clone(),
    )
    .await?;
    let resp = CreateMountResponse {
        id,
        config: body.config,
        variant: body.variant,
        path,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_update_mount(
    Extension(ctx): Extension<Context>,
    Extension(ns): Extension<Namespace>,
    Path(path): Path<String>,
    Json(body): Json<UpdateMountParams>,
) -> Result<Response, Error> {
    let me = update_mount(&ctx.repos, &path, &ns.id, body.config).await?;
    let resp = UpdateMountResponse {
        variant: me.backend_type,
        config: me.config,
        id: me.id,
        path,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_mounts_list(
    Extension(ns): Extension<Namespace>,
    Extension(ctx): Extension<Context>,
) -> Result<Response, Error> {
    let mut auth = vec![];
    let mut secret = vec![];

    let mounts = ctx.repos.mount.list(&ns.id).await?;

    for mount in mounts {
        let mount = MountsListItemResponse {
            id: mount.id,
            path: mount.path,
            // TODO: remove category
            category: mount.backend_type.into(),
            variant: mount.backend_type,
            config: mount.config,
        };
        match mount.category {
            BackendCategory::Credential => auth.push(mount),
            BackendCategory::Logical => secret.push(mount),
        }
    }

    let resp = MountsListResponse { auth, secret };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_mount_disable(
    Extension(ctx): Extension<Context>,
    Extension(ns): Extension<Namespace>,
    Path(path): Path<String>,
) -> Result<Response, Error> {
    let mount = remove_mount(&ctx, &path, &ns.id).await?;
    let resp = DisableMountResponse {
        mount: MountsListItemResponse {
            id: mount.id,
            path,
            category: mount.backend_type.into(),
            variant: mount.backend_type,
            config: mount.config,
        },
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

async fn update_mount(
    repos: &Repos,
    path: &str,
    namespace_id: &str,
    config: MountConfig,
) -> Result<MountEntry, Error> {
    let mut me = repos
        .mount
        .get_by_path(path, namespace_id)
        .await?
        .ok_or_else(|| ErrorType::MountNotFound { path: path.into() })?;
    me.config = config;
    repos
        .mount
        .set_config(&me.path, namespace_id, &me.config)
        .await?;

    Ok(me)
}

#[tracing::instrument(skip(ctx))]
pub async fn remove_mount(
    ctx: &Context,
    path: &str,
    namespace_id: &str,
) -> Result<MountEntry, Error> {
    let me = ctx
        .repos
        .mount
        .get_by_path(path, namespace_id)
        .await?
        .ok_or_else(|| ErrorType::MountNotFound { path: path.into() })?;

    // TODO: system backend isn't actually stored in mount store
    // Same check as above just for good measure!
    if me.backend_type == BackendType::System {
        return Err(ErrorType::InvalidMountType {
            variant: BackendType::System,
        }
        .into());
    }

    ctx.expiration_manager
        .revoke_leases_by_mount_prefix(path, namespace_id)
        .await?;
    if !ctx.router.remove(me.id) {
        return Err(ErrorType::MountNotFound { path: path.into() }.into());
    }
    ctx.repos.mount.remove_by_path(path, namespace_id).await?;

    // Delete all storage for the mount
    let namespace_id = Uuid::from_str(namespace_id).map_err(|_| {
        ErrorType::InternalError(anyhow::Error::msg("Namespace id was not a valid UUID"))
    })?;
    let storage = storage_pool_for_backend(
        Arc::clone(&ctx.repos.pool),
        namespace_id,
        me.backend_type,
        me.id,
    );
    let storage_prefix = storage.prefix();
    let tables =
        crate::helpers::sqlite::get_resources_by_prefix(ctx.repos.pool.as_ref(), storage_prefix)
            .await?;

    for table in tables {
        info!("Dropping table {}", table.name);
        crate::helpers::sqlite::drop_table(ctx.repos.pool.as_ref(), &table.name).await?;
    }

    Ok(me)
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub async fn mount_route_entry(
    ctx: &Context,
    id: Uuid,
    variant: BackendType,
    namespace_id: &str,
) -> Result<(Arc<Backend>, String), Error> {
    let namespace_id = Uuid::from_str(namespace_id).map_err(|_| {
        ErrorType::InternalError(anyhow::Error::msg("Namespace id was not a valid UUID"))
    })?;
    let backend_storage =
        storage_pool_for_backend(Arc::clone(&ctx.repos.pool), namespace_id, variant, id);

    let prefix = backend_storage.prefix().to_string();
    let backend = Arc::new(new_backend(ctx, backend_storage, variant).await?);

    ctx.router.mount(id, Arc::clone(&backend));

    Ok((backend, prefix))
}

async fn new_backend(
    ctx: &Context,
    storage: BackendStoragePool,
    variant: BackendType,
) -> Result<Backend, MigrationError> {
    match variant {
        BackendType::Kv => new_versioned_kv_backend(storage),
        BackendType::Postgres => new_psql_backend(storage).await,
        BackendType::System => Ok(new_system_backend(ctx.clone())),
        BackendType::Userpass => new_userpass_backend(storage),
    }
}

/// Mount a new backend
#[tracing::instrument(skip(ctx))]
pub async fn mount(
    ctx: &Context,
    path: String,
    namespace_id: String,
    variant: BackendType,
    mount_config: MountConfig,
) -> Result<Uuid, Error> {
    if variant == BackendType::System {
        return Err(ErrorType::InvalidMountType {
            variant: BackendType::System,
        })?;
    }

    if path.starts_with(SYSTEM_MOUNT_PATH) || SYSTEM_MOUNT_PATH.starts_with(&path) {
        return Err(ErrorType::MountPathConflict {
            path,
            existing_path: SYSTEM_MOUNT_PATH.to_string(),
        }
        .into());
    }

    // Check if conflicting path exist
    // TODO: this should take a lock on mount creation
    if let Some(mount) = ctx.repos.mount.longest_prefix(&path, &namespace_id).await? {
        return Err(ErrorType::MountPathConflict {
            path,
            existing_path: mount.path,
        }
        .into());
    }

    let is_auth_path = path.starts_with("auth/");
    let is_auth_backend = BackendCategory::from(variant) == BackendCategory::Credential;
    if is_auth_backend != is_auth_path {
        if is_auth_backend {
            return Err(ErrorType::AuthBackendNotUnderAuthPath)?;
        }

        return Err(ErrorType::LogicalBackendUnderAuthPath)?;
    }

    // Mount internally
    let uuid = Uuid::new_v4();
    let (backend, prefix) = mount_route_entry(ctx, uuid, variant, &namespace_id).await?;

    let entry = MountEntry {
        id: uuid,
        path,
        config: mount_config,
        backend_type: variant,
        namespace_id,
    };
    // TODO: remove entry from the internal router if it fails to store in db
    ctx.repos.mount.create(&entry).await?;

    if !backend.migrations.is_empty() {
        backend
            .migrate(Arc::clone(&ctx.repos.pool), &uuid.to_string(), &prefix)
            .await
            .map_err(|error| ErrorType::BackendMigration { error, variant })?;
    }

    Ok(uuid)
}

pub fn storage_pool_for_backend(
    pool: Arc<EncryptedPool>,
    namespace_id: Uuid,
    variant: BackendType,
    id: Uuid,
) -> BackendStoragePool {
    let id = id.to_simple();
    let ns_id = namespace_id.to_simple();

    // Prefix with "covert_" because it cannot start with a digit and namespace id
    // possibly starts with a digit.
    let prefix = format!("covert_{ns_id}_{variant}_{id}_");

    BackendStoragePool::new(&prefix, pool)
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;
    use tokio::sync::broadcast;

    use crate::{
        expiration_manager::clock::SystemClock, repos::mount::tests::pool, Config,
        ExpirationManager, Router,
    };

    use super::*;

    async fn create_context() -> Context {
        let pool = Arc::new(pool().await);
        let u_pool = SqlitePool::connect(":memory:").await.unwrap();
        let repos = Repos::new(pool, u_pool);
        let router = Arc::new(Router::new(repos.mount.clone()));

        let (stop_tx, _stop_rx) = broadcast::channel(1);
        Context {
            config: Arc::new(Config {
                port: 0,
                port_tx: None,
                replication: None,
                storage_path: String::new(),
            }),
            expiration_manager: Arc::new(ExpirationManager::new(
                router.clone(),
                repos.clone(),
                SystemClock {},
            )),
            repos,
            router,
            stop_tx,
        }
    }

    #[tokio::test]
    async fn auth_backend_needs_to_be_mounted_under_auth_path() {
        let ctx = create_context().await;

        let path = "not-auth/".to_string();
        let namespace_id = Uuid::new_v4().to_string();
        let variant = BackendType::Userpass;
        let err = mount(&ctx, path, namespace_id, variant, MountConfig::default())
            .await
            .unwrap_err();
        assert!(matches!(
            err.variant,
            ErrorType::AuthBackendNotUnderAuthPath
        ));
    }

    #[tokio::test]
    async fn secret_engine_cannot_be_mounted_under_auth_path() {
        let ctx = create_context().await;

        let path = "auth/kv/".to_string();
        let namespace_id = Uuid::new_v4().to_string();
        let variant = BackendType::Kv;
        let err = mount(&ctx, path, namespace_id, variant, MountConfig::default())
            .await
            .unwrap_err();
        assert!(matches!(
            err.variant,
            ErrorType::LogicalBackendUnderAuthPath
        ));
    }

    #[tokio::test]
    async fn system_backend_cannot_be_externally_mounted() {
        let ctx = create_context().await;

        let path = "new-sys/".to_string();
        let namespace_id = Uuid::new_v4().to_string();
        let variant = BackendType::System;
        let err = mount(&ctx, path, namespace_id, variant, MountConfig::default())
            .await
            .unwrap_err();
        assert!(matches!(
            err.variant,
           ErrorType::InvalidMountType { variant } if variant == BackendType::System
        ));
    }

    #[tokio::test]
    async fn cannot_mount_at_path_that_collides_with_sys() {
        let ctx = create_context().await;

        let bad_paths = ["", "sy", "sys", "sys/", "sys/new/"];

        for path in bad_paths {
            let namespace_id = Uuid::new_v4().to_string();
            let variant = BackendType::Kv;
            let err = mount(
                &ctx,
                path.to_string(),
                namespace_id,
                variant,
                MountConfig::default(),
            )
            .await
            .unwrap_err();
            assert!(matches!(
                err.variant,
               ErrorType::MountPathConflict { existing_path, .. } if existing_path == SYSTEM_MOUNT_PATH
            ));
        }
    }
}
