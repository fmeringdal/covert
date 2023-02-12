use std::sync::Arc;

use covert_framework::extract::{Extension, Json};
use covert_types::{response::Response, token::Token};
use serde::{Deserialize, Serialize};

use crate::{error::Error, store::token_store::TokenStore};

#[derive(Debug, Deserialize, Serialize)]
pub struct RevokeTokenParams {
    pub token: Token,
}

pub async fn handle_token_revocation(
    Extension(token_store): Extension<Arc<TokenStore>>,
    Json(body): Json<RevokeTokenParams>,
) -> Result<Response, Error> {
    token_store.remove(&body.token).await?;
    Ok(Response::ok())
}
