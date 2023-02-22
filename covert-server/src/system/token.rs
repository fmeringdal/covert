use chrono::Utc;
use covert_framework::extract::{Extension, Json};
use covert_types::{
    methods::{psql::RenewLeaseResponse, RenewLeaseParams},
    response::Response,
    token::Token,
};
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, ErrorType},
    repos::{namespace::Namespace, Repos},
};

#[derive(Debug, Deserialize, Serialize)]
pub struct RevokeTokenParams {
    pub token: Token,
}

#[tracing::instrument(skip_all)]
pub async fn handle_token_revocation(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Json(body): Json<RevokeTokenParams>,
) -> Result<Response, Error> {
    repos.token.remove(&body.token, &ns.id).await?;
    Ok(Response::ok())
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RenewTokenParams {
    pub token: Token,
}

#[tracing::instrument(skip_all)]
pub async fn handle_token_renewal(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Json(body): Json<RenewLeaseParams<String>>,
) -> Result<Response, Error> {
    let data: RenewTokenParams = serde_json::from_str(&body.data).map_err(|_| {
        ErrorType::InternalError(anyhow::Error::msg(
            "Token renew request failed to deserialize",
        ))
    })?;

    let ttl = chrono::Duration::from_std(body.ttl)
        .map_err(|_| ErrorType::InternalError(anyhow::Error::msg("Unable to create TTL")))?;
    let expires_at = Utc::now() + ttl;

    repos.token.renew(&data.token, &ns.id, expires_at).await?;

    let resp = RenewLeaseResponse { ttl: body.ttl };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
