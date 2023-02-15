use std::sync::Arc;

use covert_framework::extract::{Extension, Json};
use covert_types::{
    methods::kv::{ReadConfigResponse, SetConfigParams, SetConfigResponse},
    response::Response,
};

use crate::{domain::config::Configuration, error::Error};

use super::Context;

#[tracing::instrument(skip(ctx))]
pub async fn set_config(
    Extension(ctx): Extension<Arc<Context>>,
    Json(body): Json<SetConfigParams>,
) -> Result<Response, Error> {
    let config = Configuration {
        max_versions: body.max_versions,
    };
    ctx.repos.config.set(&config).await?;
    let resp = SetConfigResponse {
        max_versions: config.max_versions,
    };
    Response::raw(resp).map_err(Into::into)
}

#[tracing::instrument(skip_all)]
pub async fn read_config(Extension(ctx): Extension<Arc<Context>>) -> Result<Response, Error> {
    let config = ctx.repos.config.load().await?;

    let resp = ReadConfigResponse {
        max_versions: config.max_versions,
    };

    Response::raw(resp).map_err(Into::into)
}
