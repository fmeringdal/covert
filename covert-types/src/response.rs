use std::time::Duration;

use http::StatusCode;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tracing_error::SpanTrace;

use crate::error::ApiError;

/// Response from the backend
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Response {
    /// Raw response. The data will be returned as is to the client.
    Raw(Value),
    /// Authentication response. The client will receive a token for the
    /// given alias.
    Auth(AuthResponse),
    /// Register a lease for the payload. Useful for returning dynamic
    /// secrets that can be revoked and renewed.
    Lease(LeaseResponse),
}

// TODO: add renew fields as well
#[derive(Debug, Serialize)]
pub struct LeaseResponse {
    pub revoke: LeaseRenewRevokeEndpoint,
    pub renew: LeaseRenewRevokeEndpoint,
    pub data: Value,
    #[serde(with = "humantime_serde")]
    pub ttl: Option<Duration>,
}

#[derive(Debug, Serialize)]
pub struct LeaseRenewRevokeEndpoint {
    pub path: String,
    pub data: Value,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub alias: String,
    #[serde(with = "humantime_serde")]
    pub ttl: Option<Duration>,
}

impl Response {
    #[must_use]
    pub fn ok() -> Self {
        Self::Raw(Value::default())
    }

    /// Construct a response with data that will be returned as is to the client.
    ///
    /// # Errors
    ///
    /// Returns an error if it fails to serialize the payload.
    pub fn raw<T: Serialize>(data: T) -> Result<Self, serde_json::Error> {
        serde_json::to_value(data).map(Self::Raw)
    }

    /// Try to deserialize the raw data payload from the response.
    ///
    /// # Errors
    ///
    /// Returns an error if it fails to deserialize the raw payload or if the
    /// response is not a raw payload.
    pub fn data<T: DeserializeOwned>(self) -> Result<T, ApiError> {
        match self {
            Response::Raw(data) => serde_json::from_value(data).map_err(|err| ApiError {
                error: err.into(),
                status_code: StatusCode::BAD_REQUEST,
                span_trace: Some(SpanTrace::capture()),
            }),
            Response::Auth(_data) => Err(ApiError {
                error: anyhow::Error::msg("expected raw data, found auth data"),
                status_code: StatusCode::BAD_REQUEST,
                span_trace: Some(SpanTrace::capture()),
            }),
            Response::Lease(_data) => Err(ApiError {
                error: anyhow::Error::msg("expected raw data, found lease data"),
                status_code: StatusCode::BAD_REQUEST,
                span_trace: Some(SpanTrace::capture()),
            }),
        }
    }
}
