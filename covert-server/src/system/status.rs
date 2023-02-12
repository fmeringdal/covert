use std::sync::Arc;

use covert_framework::extract::Extension;
use covert_types::{methods::system::StatusResponse, response::Response};

use crate::{
    error::{Error, ErrorType},
    Core,
};

#[allow(clippy::unused_async)]
pub async fn handle_status(Extension(core): Extension<Arc<Core>>) -> Result<Response, Error> {
    let resp = StatusResponse {
        state: core.state(),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
