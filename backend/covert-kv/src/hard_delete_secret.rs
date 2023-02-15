use std::sync::Arc;

use covert_framework::extract::{Extension, Json, Path};
use covert_types::{
    methods::kv::{HardDeleteSecretParams, HardDeleteSecretResponse},
    response::Response,
};

use crate::error::{Error, ErrorType};

use super::Context;

#[tracing::instrument(skip_all)]
pub async fn hard_delete_secret(
    Extension(ctx): Extension<Arc<Context>>,
    Path(key): Path<String>,
    Json(body): Json<HardDeleteSecretParams>,
) -> Result<Response, Error> {
    if body.versions.is_empty() {
        return Err(ErrorType::MissingKeyVersions.into());
    }

    let not_deleted = ctx.repos.secrets.hard_delete(&key, &body.versions).await?;

    let resp = HardDeleteSecretResponse { not_deleted };
    Response::raw(resp).map_err(Into::into)
}
