use std::fmt::Display;

use covert_types::error::{ApiError, StatusCode};
use thiserror::Error;
use tracing_error::SpanTrace;

#[derive(Error, Debug)]
pub enum ErrorType {
    #[error("Internal error")]
    Storage(#[from] sqlx::Error),
    #[error("Bad request")]
    BadRequest(#[from] serde_json::Error),
    #[error("User with username: `{username}` not found")]
    UserNotFound { username: String },
    #[error("Incorrect password")]
    IncorrectPassword,
    #[error("Unsupported password")]
    UnsupportedPassword,
}

#[derive(Error, Debug)]
pub struct Error {
    pub variant: ErrorType,
    pub span_trace: SpanTrace,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\n{}", self.variant, self.span_trace)
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Self {
            variant: err.into(),
            span_trace: SpanTrace::capture(),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self {
            variant: err.into(),
            span_trace: SpanTrace::capture(),
        }
    }
}

impl From<ErrorType> for Error {
    fn from(err: ErrorType) -> Self {
        Self {
            variant: err,
            span_trace: SpanTrace::capture(),
        }
    }
}

impl From<Error> for ApiError {
    fn from(err: Error) -> Self {
        let status_code = match err.variant {
            ErrorType::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorType::BadRequest(_) | ErrorType::UnsupportedPassword => StatusCode::BAD_REQUEST,
            ErrorType::UserNotFound { .. } => StatusCode::NOT_FOUND,
            ErrorType::IncorrectPassword { .. } => StatusCode::UNAUTHORIZED,
        };

        ApiError {
            error: err.variant.into(),
            status_code,
            span_trace: Some(err.span_trace),
        }
    }
}
