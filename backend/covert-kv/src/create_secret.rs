use std::sync::Arc;

use chrono::Utc;

use super::Context;
use crate::{
    domain::secret::Secret,
    error::{Error, ErrorType},
};
use covert_framework::extract::{Extension, Json, Path, Query};
use covert_types::{
    methods::kv::{CreateSecretParams, CreateSecretResponse, ReadSecretQuery, ReadSecretResponse},
    response::Response,
};

#[tracing::instrument(skip_all)]
pub async fn add_secret(
    Extension(ctx): Extension<Arc<Context>>,
    Path(key): Path<String>,
    Json(body): Json<CreateSecretParams>,
) -> Result<Response, Error> {
    let version_metadata = ctx.repos.secrets.version_metadata(&key).await?;

    let value = serde_json::to_string(&body.data)?;
    let secret = Secret {
        key: key.clone(),
        version: version_metadata.map_or(0, |v| v.max_version + 1),
        value: Some(value),
        created_time: Utc::now(),
        deleted: false,
        destroyed: false,
    };
    ctx.repos.secrets.insert(&secret).await?;

    let config = ctx.repos.config.load().await?;
    ctx.repos
        .secrets
        .prune_old_versions(&key, config.max_versions)
        .await?;

    let version_metadata = ctx
        .repos
        .secrets
        .version_metadata(&key)
        .await?
        .ok_or_else(|| {
            ErrorType::InternalError(anyhow::Error::msg(
                "Metadata for key should not be null when a new version has just been added",
            ))
        })?;

    let resp = CreateSecretResponse {
        version: secret.version,
        created_time: secret.created_time,
        deleted: secret.deleted,
        destroyed: secret.destroyed,
        min_version: version_metadata.min_version,
        max_version: version_metadata.max_version,
    };
    Response::raw(resp).map_err(Into::into)
}

#[tracing::instrument(skip_all)]
pub async fn read_secret(
    Extension(ctx): Extension<Arc<Context>>,
    Path(key): Path<String>,
    Query(query): Query<ReadSecretQuery>,
) -> Result<Response, Error> {
    let version_metadata = ctx
        .repos
        .secrets
        .version_metadata(&key)
        .await?
        .ok_or(ErrorType::MetadataNotFound)?;

    let version = query.version.unwrap_or(version_metadata.max_version);
    let secret = ctx
        .repos
        .secrets
        .get(&key, version)
        .await?
        .ok_or(ErrorType::KeyVersionNotFound)?;

    let mut resp = ReadSecretResponse {
        data: None,
        metadata: CreateSecretResponse {
            version,
            min_version: version_metadata.min_version,
            max_version: version_metadata.max_version,
            created_time: secret.created_time,
            deleted: secret.deleted,
            destroyed: secret.destroyed,
        },
    };

    if !secret.deleted && !secret.destroyed {
        resp.data = secret
            .value
            .as_ref()
            .map(|value| serde_json::from_str(value))
            .transpose()?;
    }

    Response::raw(resp).map_err(Into::into)
}
