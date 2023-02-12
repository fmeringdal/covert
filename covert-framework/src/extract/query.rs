use std::ops::Deref;

use covert_types::error::ApiError;
use serde::de::DeserializeOwned;

use super::{FromRequest, Request};

#[derive(Debug)]
pub struct Query<T>(pub T);

impl<T> Deref for Query<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: DeserializeOwned> FromRequest for Query<T> {
    #[tracing::instrument(level = "debug", name = "query_string_extractor", skip_all)]
    fn from_request(req: &mut Request) -> Result<Self, ApiError> {
        serde_qs::from_str(&req.query_string)
            .map(Query)
            .map_err(|_| ApiError::bad_request())
    }
}
