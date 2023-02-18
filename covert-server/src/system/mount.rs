use std::sync::Arc;

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
    repos::Repos,
    router::{RouteEntry, TrieMount},
    ExpirationManager, Router,
};

use super::new_system_backend;

#[tracing::instrument(skip(repos, expiration_manager, router))]
pub async fn handle_mount(
    Extension(repos): Extension<Repos>,
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
        body.variant,
        body.config.clone(),
        false,
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
    Extension(router): Extension<Arc<Router>>,
    Path(path): Path<String>,
    Json(body): Json<UpdateMountParams>,
) -> Result<Response, Error> {
    let me = update_mount(&repos, &router, &path, body.config).await?;
    let resp = UpdateMountResponse {
        variant: me.backend_type,
        config: me.config,
        id: me.id,
        path,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_mounts_list(
    Extension(router): Extension<Arc<Router>>,
) -> Result<Response, Error> {
    let mut auth = vec![];
    let mut secret = vec![];

    for mount in router.mounts().await {
        let TrieMount { path, value: re } = mount;

        let mount = MountsListItemResponse {
            id: re.id(),
            path,
            category: re.backend().category(),
            variant: re.backend().variant(),
            config: re.config_cloned().clone(),
        };
        match re.backend().category() {
            BackendCategory::Credential => auth.push(mount),
            BackendCategory::Logical => secret.push(mount),
        }
    }

    let resp = MountsListResponse { auth, secret };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_mount_disable(
    Extension(repos): Extension<Repos>,
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(router): Extension<Arc<Router>>,
    Path(path): Path<String>,
) -> Result<Response, Error> {
    let mount = remove_mount(&repos, &router, &expiration_manager, &path).await?;
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
    router: &Router,
    path: &str,
    config: MountConfig,
) -> Result<MountEntry, Error> {
    let mut me = repos
        .mount
        .get_by_path(path)
        .await?
        .ok_or_else(|| ErrorType::MountNotFound { path: path.into() })?;
    me.config = config;
    repos.mount.set_config(me.id, &me.config).await?;
    router.update_mount(path, me.config.clone()).await?;

    Ok(me)
}

#[tracing::instrument(skip(repos, router, expiration_manager))]
pub async fn remove_mount(
    repos: &Repos,
    router: &Router,
    expiration_manager: &ExpirationManager,
    path: &str,
) -> Result<MountEntry, Error> {
    // In case it is not in the router we will still try to remove it from the
    // mounts store
    if let Some(re) = router.get(path).await {
        if re.backend().variant() == BackendType::System {
            return Err(ErrorType::InvalidMountType {
                variant: BackendType::System,
            }
            .into());
        }
    }

    let me = repos
        .mount
        .get_by_path(path)
        .await?
        .ok_or_else(|| ErrorType::MountNotFound { path: path.into() })?;

    // Same check as above just for good measure!
    if me.backend_type == BackendType::System {
        return Err(ErrorType::InvalidMountType {
            variant: BackendType::System,
        }
        .into());
    }

    expiration_manager
        .revoke_leases_by_mount_prefix(path)
        .await?;
    if !router.remove(path).await {
        return Err(ErrorType::MountNotFound { path: path.into() }.into());
    }
    repos.mount.remove_by_path(path).await?;

    // Delete all storage for the mount
    let storage = storage_pool_for_backend(Arc::clone(&repos.pool), me.backend_type, &me.id);
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
    path: String,
    id: Uuid,
    variant: BackendType,
    config: MountConfig,
) -> Result<(Arc<Backend>, String), Error> {
    let backend_storage = storage_pool_for_backend(Arc::clone(&repos.pool), variant, &id);

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

    let re = RouteEntry::new(id, path, Arc::clone(&backend), config)?;
    router.mount(re).await?;

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
    variant: BackendType,
    config: MountConfig,
    internal: bool,
) -> Result<Uuid, Error> {
    // Mount internally
    let uuid = Uuid::new_v4();
    let (backend, prefix) = mount_route_entry(
        repos,
        expiration_manager,
        router,
        path.clone(),
        uuid,
        variant,
        config.clone(),
    )
    .await?;

    let is_internal_backend = matches!(variant, BackendType::System);

    if is_internal_backend && !internal {
        return Err(ErrorType::InvalidMountType { variant }.into());
    }

    if !is_internal_backend {
        let entry = MountEntry {
            id: uuid,
            path,
            config,
            backend_type: variant,
        };
        // TODO: remove entry from the internal router if it fails to store in db
        repos.mount.create(&entry).await?;
    }
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
    variant: BackendType,
    id: &Uuid,
) -> BackendStoragePool {
    let id = id.to_simple();
    let prefix = format!("{variant}_{id}_");
    BackendStoragePool::new(&prefix, pool)
}
