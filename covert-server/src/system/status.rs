use covert_framework::extract::Extension;
use covert_types::{methods::system::StatusResponse, response::Response};

use crate::{
    context::Context,
    error::{Error, ErrorType},
};

#[allow(clippy::unused_async)]
pub async fn handle_status(Extension(ctx): Extension<Context>) -> Result<Response, Error> {
    let resp = StatusResponse {
        state: ctx.repos.pool.state(),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
