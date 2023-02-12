use std::sync::Arc;

use covert_framework::extract::{Extension, Json};
use covert_types::{
    methods::system::{UnsealParams, UnsealResponse},
    response::Response,
};

use crate::{
    error::{Error, ErrorType},
    Core,
};

pub async fn handle_unseal(
    Extension(core): Extension<Arc<Core>>,
    Json(body): Json<UnsealParams>,
) -> Result<Response, Error> {
    let threshold = u8::try_from(body.shares.len())
        .map_err(|_| ErrorType::BadRequest("Invalid number of shares".into()))?;

    let key_shares = body
        .shares
        .iter()
        .map(|s| {
            hex::decode(s)
                .map_err(|_| ErrorType::BadRequest("Malformed key shares".into()))
                .and_then(|share| {
                    sharks::Share::try_from(share.as_slice())
                        .map_err(|_| ErrorType::BadRequest("Malformed key shares".into()))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sharks = sharks::Sharks(threshold);
    let master_key = sharks
        .recover(key_shares.as_slice())
        .map_err(|_| ErrorType::MasterKeyRecovery)?;
    let master_key = String::from_utf8(master_key).map_err(|_| ErrorType::MasterKeyRecovery)?;

    core.unseal(master_key).await?;

    let root_token = core.generate_root_token().await?;

    let resp = UnsealResponse { root_token };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
