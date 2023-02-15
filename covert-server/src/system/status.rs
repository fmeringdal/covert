use covert_framework::extract::Extension;
use covert_types::{methods::system::StatusResponse, response::Response};

use crate::{
    error::{Error, ErrorType},
    repos::Repos,
};

#[allow(clippy::unused_async)]
pub async fn handle_status(Extension(repos): Extension<Repos>) -> Result<Response, Error> {
    let resp = StatusResponse {
        state: repos.pool.state(),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
