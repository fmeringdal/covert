use std::sync::Arc;

use covert_framework::extract::{Extension, Json, Path};
use covert_types::{
    backend::BackendCategory,
    methods::system::{
        CreateMountParams, CreateMountResponse, DisbaleMountResponse, MountsListItemResponse,
        MountsListResponse, UpdateMountParams, UpdateMountResponse,
    },
    request::Operation,
    response::Response,
};

use crate::{
    error::{Error, ErrorType},
    layer::auth_service::Permissions,
    router::TrieMount,
    Core,
};

pub async fn handle_mount(
    Extension(core): Extension<Arc<Core>>,
    Path(path): Path<String>,
    Json(body): Json<CreateMountParams>,
) -> Result<Response, Error> {
    let id = core
        .mount(path.clone(), body.variant, body.config.clone(), false)
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
    Extension(core): Extension<Arc<Core>>,
    Path(path): Path<String>,
    Json(body): Json<UpdateMountParams>,
) -> Result<Response, Error> {
    let me = core.update_mount(&path, body.config).await?;
    let resp = UpdateMountResponse {
        variant: me.backend_type,
        config: me.config,
        id: me.id,
        path,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_mounts_list(
    Extension(core): Extension<Arc<Core>>,
    Extension(permissions): Extension<Permissions>,
) -> Result<Response, Error> {
    let mut auth = vec![];
    let mut secret = vec![];

    for mount in core.router().mounts().await {
        let TrieMount { path, value: re } = mount;

        match &permissions {
            Permissions::Root => (),
            Permissions::Unauthenticated => continue,
            Permissions::Authenticated(policies) => {
                if !policies
                    .iter()
                    .any(|policy| policy.is_authorized(&path, &[Operation::Read]))
                {
                    continue;
                }
            }
        }

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
    Extension(core): Extension<Arc<Core>>,
    Path(path): Path<String>,
) -> Result<Response, Error> {
    let mount = core.remove_mount(&path).await?;
    let resp = DisbaleMountResponse {
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
