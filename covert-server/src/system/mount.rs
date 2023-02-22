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
    error::{Error, ErrorType},
    repos::{namespace::Namespace, Repos},
    ExpirationManager, Router,
};

use super::{new_system_backend, SYSTEM_MOUNT_PATH};

#[tracing::instrument(skip(repos, expiration_manager, router))]
pub async fn handle_mount(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Extension(router): Extension<Arc<Router>>,
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Path(path): Path<String>,
    Json(body): Json<CreateMountParams>,
) -> Result<Response, Error> {
    let id = mount(
        &repos,
        expiration_manager,
        router,
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
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Path(path): Path<String>,
    Json(body): Json<UpdateMountParams>,
) -> Result<Response, Error> {
    let me = update_mount(&repos, &path, &ns.id, body.config).await?;
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
    Extension(repos): Extension<Repos>,
) -> Result<Response, Error> {
    let mut auth = vec![];
    let mut secret = vec![];

    let mounts = repos.mount.list(&ns.id).await?;

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
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(router): Extension<Arc<Router>>,
    Path(path): Path<String>,
) -> Result<Response, Error> {
    let mount = remove_mount(&repos, &router, &expiration_manager, &path, &ns.id).await?;
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

#[tracing::instrument(skip(repos, router, expiration_manager))]
pub async fn remove_mount(
    repos: &Repos,
    router: &Router,
    expiration_manager: &ExpirationManager,
    path: &str,
    namespace_id: &str,
) -> Result<MountEntry, Error> {
    let me = repos
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

    expiration_manager
        .revoke_leases_by_mount_prefix(path, namespace_id)
        .await?;
    if !router.remove(me.id).await {
        return Err(ErrorType::MountNotFound { path: path.into() }.into());
    }
    repos.mount.remove_by_path(path, namespace_id).await?;

    // Delete all storage for the mount
    let namespace_id = Uuid::from_str(namespace_id).map_err(|_| {
        ErrorType::InternalError(anyhow::Error::msg("Namespace id was not a valid UUID"))
    })?;
    let storage = storage_pool_for_backend(
        Arc::clone(&repos.pool),
        namespace_id,
        me.backend_type,
        me.id,
    );
    let storage_prefix = storage.prefix();
    let tables =
        crate::helpers::sqlite::get_resources_by_prefix(repos.pool.as_ref(), storage_prefix)
            .await?;

    for table in tables {
        info!("Dropping table {}", table.name);
        crate::helpers::sqlite::drop_table(repos.pool.as_ref(), &table.name).await?;
    }

    Ok(me)
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub async fn mount_route_entry(
    repos: &Repos,
    expiration_manager: Arc<ExpirationManager>,
    router: Arc<Router>,
    id: Uuid,
    variant: BackendType,
    namespace_id: &str,
) -> Result<(Arc<Backend>, String), Error> {
    let namespace_id = Uuid::from_str(namespace_id).map_err(|_| {
        ErrorType::InternalError(anyhow::Error::msg("Namespace id was not a valid UUID"))
    })?;
    let backend_storage =
        storage_pool_for_backend(Arc::clone(&repos.pool), namespace_id, variant, id);

    let prefix = backend_storage.prefix().to_string();
    let backend = Arc::new(
        new_backend(
            repos,
            Arc::clone(&router),
            expiration_manager,
            backend_storage,
            variant,
        )
        .await?,
    );

    router.mount(id, Arc::clone(&backend)).await;

    Ok((backend, prefix))
}

async fn new_backend(
    repos: &Repos,
    router: Arc<Router>,
    expiration_manager: Arc<ExpirationManager>,
    storage: BackendStoragePool,
    variant: BackendType,
) -> Result<Backend, MigrationError> {
    match variant {
        BackendType::Kv => new_versioned_kv_backend(storage),
        BackendType::Postgres => new_psql_backend(storage).await,
        BackendType::System => Ok(new_system_backend(
            repos.clone(),
            router,
            expiration_manager,
        )),
        BackendType::Userpass => new_userpass_backend(storage),
    }
}

/// Mount a new backend
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(repos, expiration_manager, router))]
pub async fn mount(
    repos: &Repos,
    expiration_manager: Arc<ExpirationManager>,
    router: Arc<Router>,
    path: String,
    namespace_id: String,
    variant: BackendType,
    config: MountConfig,
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
    if let Some(mount) = repos.mount.longest_prefix(&path, &namespace_id).await? {
        return Err(ErrorType::MountPathConflict {
            path,
            existing_path: mount.path,
        }
        .into());
    }

    // Mount internally
    let uuid = Uuid::new_v4();
    let (backend, prefix) = mount_route_entry(
        repos,
        expiration_manager,
        router,
        uuid,
        variant,
        &namespace_id,
    )
    .await?;

    let entry = MountEntry {
        id: uuid,
        path,
        config,
        backend_type: variant,
        namespace_id,
    };
    // TODO: remove entry from the internal router if it fails to store in db
    repos.mount.create(&entry).await?;

    if !backend.migrations.is_empty() {
        backend
            .migrate(Arc::clone(&repos.pool), &uuid.to_string(), &prefix)
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
