use covert_types::{error::ApiError, mount::MountConfig, response::Response};
use hyper::{header::CONTENT_TYPE, Body, StatusCode};
use serde::Serialize;
use uuid::Uuid;

use crate::error::{Error, ErrorType};

#[derive(Debug, Serialize, Default)]
pub struct ResponseContext {
    pub backend_mount_path: String,
    pub backend_config: MountConfig,
    pub backend_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct ResponseWithCtx {
    #[serde(rename = "data")]
    pub response: Response,
    #[serde(skip)]
    pub ctx: ResponseContext,
}

impl From<ResponseWithCtx> for hyper::Response<Body> {
    fn from(resp: ResponseWithCtx) -> Self {
        match serde_json::to_vec(&resp) {
            Ok(body) => match hyper::Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "application/json")
                .body(body.into())
            {
                Ok(resp) => resp,
                Err(err) => ApiError::from(Error::from(ErrorType::BadHttpResponseData(err))).into(),
            },
            Err(err) => ApiError::from(Error::from(ErrorType::BadResponseData(err))).into(),
        }
    }
}
