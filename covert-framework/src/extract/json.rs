use std::ops::Deref;

use covert_types::error::ApiError;
use serde::de::DeserializeOwned;
use tracing::debug;

use super::{FromRequest, Request};

#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T> Deref for Json<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: DeserializeOwned> FromRequest for Json<T> {
    #[tracing::instrument(level = "debug", name = "json_extractor", skip_all)]
    fn from_request(req: &mut Request) -> Result<Self, ApiError> {
        serde_json::from_slice(&req.data).map(Json).map_err(|_| {
            let expected_type_name = std::any::type_name::<T>();
            debug!(data = ?req.data, expected_type_name, "JSON extraction failed");
            ApiError::bad_request()
        })
    }
}
