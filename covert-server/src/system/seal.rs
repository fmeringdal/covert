use std::sync::Arc;

use covert_framework::extract::Extension;
use covert_types::{methods::system::SealResponse, response::Response};

use crate::{
    error::{Error, ErrorType},
    Core,
};

pub async fn handle_seal(Extension(core): Extension<Arc<Core>>) -> Result<Response, Error> {
    core.seal().await?;

    let resp = SealResponse {
        message: "Successfully sealed".into(),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
