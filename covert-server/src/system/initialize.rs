use std::sync::Arc;

use covert_framework::extract::{Extension, Json};
use covert_types::{
    methods::system::{
        InitializeParams, InitializeResponse, InitializedKeyShares, InitializedWithExistingKey,
    },
    response::Response,
};

use crate::{
    error::{Error, ErrorType},
    Core,
};

#[allow(clippy::unused_async)]
pub async fn handle_initialize(
    Extension(core): Extension<Arc<Core>>,
    Json(body): Json<InitializeParams>,
) -> Result<Response, Error> {
    // Sanity check params before making real master key
    if body.threshold == 0 || body.shares < body.threshold {
        return Err(ErrorType::InvalidInitializeParams.into());
    }

    if let Some(master_key) = core.initialize()? {
        let sharks = sharks::Sharks(body.threshold);
        let key_shares = sharks
            .dealer(master_key.as_bytes())
            .map(|key_share| hex::encode(Vec::<u8>::from(&key_share)))
            .take(usize::from(body.shares))
            .collect::<Vec<_>>();
        let resp = InitializeResponse::NewKeyShares(InitializedKeyShares { shares: key_shares });
        Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
    } else {
        let resp = InitializeResponse::ExistingKey(InitializedWithExistingKey {
            message: "Initialized with stored master key".into(),
        });
        Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
    }
}
