use covert_framework::extract::{Extension, Json, Path};
use covert_types::{
    methods::kv::{
        RecoverSecretParams, RecoverSecretResponse, SoftDeleteSecretParams,
        SoftDeleteSecretResponse,
    },
    response::Response,
};

use std::sync::Arc;

use crate::error::{Error, ErrorType};

use super::Context;

#[tracing::instrument(skip_all)]
pub async fn soft_delete_secret(
    Extension(ctx): Extension<Arc<Context>>,
    Path(key): Path<String>,
    Json(body): Json<SoftDeleteSecretParams>,
) -> Result<Response, Error> {
    if body.versions.is_empty() {
        return Err(ErrorType::MissingKeyVersions.into());
    }

    let not_deleted = ctx.repos.secrets.soft_delete(&key, &body.versions).await?;

    let resp = SoftDeleteSecretResponse { not_deleted };
    Response::raw(resp).map_err(Into::into)
}

#[tracing::instrument(skip_all)]
pub async fn path_undelete_write(
    Extension(ctx): Extension<Arc<Context>>,
    Path(key): Path<String>,
    Json(body): Json<RecoverSecretParams>,
) -> Result<Response, Error> {
    if body.versions.is_empty() {
        return Err(ErrorType::MissingKeyVersions.into());
    }

    let not_recovered = ctx.repos.secrets.recover(&key, &body.versions).await?;

    let resp = RecoverSecretResponse { not_recovered };
    Response::raw(resp).map_err(Into::into)
}
