use covert_framework::extract::{Extension, Json};
use covert_types::{response::Response, token::Token};
use serde::{Deserialize, Serialize};

use crate::{error::Error, repos::Repos};

#[derive(Debug, Deserialize, Serialize)]
pub struct RevokeTokenParams {
    pub token: Token,
}

#[tracing::instrument(skip_all)]
pub async fn handle_token_revocation(
    Extension(repos): Extension<Repos>,
    Json(body): Json<RevokeTokenParams>,
) -> Result<Response, Error> {
    repos.token.remove(&body.token).await?;
    Ok(Response::ok())
}
