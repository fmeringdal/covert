use std::fmt::Display;

use http::header::CONTENT_TYPE;
use serde::Serialize;
use serde_with::{serde_as, DisplayFromStr};
use thiserror::Error;

pub use http::StatusCode;
use tracing_error::SpanTrace;

use crate::state::VaultState;

/// A shares errod type used to produce public error and add additional context
/// for internal diagnostics. A public error will be produced by using the inner
/// error [`Display`] implementation and `status_code` field. The internal error
/// report will be created used the [`Debug`] implementation and `span_trace` field.
#[serde_as]
#[derive(Error, Debug, Serialize)]
pub struct ApiError {
    // Only the Display format of the source error will be returned to the client.
    #[serde_as(as = "DisplayFromStr")]
    #[source]
    pub error: anyhow::Error,
    #[serde(skip)]
    pub status_code: StatusCode,
    // TODO: make it non-optional
    #[serde(skip)]
    pub span_trace: Option<SpanTrace>,
}

impl Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let report = self.report();
        // Using Debug impl here in Display impl because ApiError
        // doesn't need the Display impl
        write!(f, "{report:?}")
    }
}

#[derive(Debug)]
pub struct Report {
    pub cause: String,
    // TODO: make it non-optional
    pub span_trace: Option<SpanTrace>,
}

impl ApiError {
    #[must_use]
    pub fn bad_request() -> Self {
        Self {
            error: anyhow::Error::msg("Bad request"),
            status_code: StatusCode::BAD_REQUEST,
            span_trace: Some(SpanTrace::capture()),
        }
    }

    #[must_use]
    pub fn internal_error() -> Self {
        Self {
            error: anyhow::Error::msg("Internal error"),
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            span_trace: Some(SpanTrace::capture()),
        }
    }

    #[must_use]
    pub fn invalid_state(current_state: VaultState) -> Self {
        Self {
            error: anyhow::Error::msg(format!(
                "This operation is not allowed when the current state is `{current_state}`"
            )),
            status_code: StatusCode::FORBIDDEN,
            span_trace: Some(SpanTrace::capture()),
        }
    }

    #[must_use]
    pub fn unauthorized() -> Self {
        Self {
            error: anyhow::Error::msg("User is not authorized to perform this operation"),
            status_code: StatusCode::UNAUTHORIZED,
            span_trace: Some(SpanTrace::capture()),
        }
    }

    #[must_use]
    pub fn not_found() -> Self {
        Self {
            error: anyhow::Error::msg("Not found"),
            status_code: StatusCode::NOT_FOUND,
            span_trace: Some(SpanTrace::capture()),
        }
    }

    #[must_use]
    pub fn report(&self) -> Report {
        Report {
            cause: format!("{:?}", self.error.root_cause()),
            span_trace: self.span_trace.clone(),
        }
    }
}

impl From<ApiError> for hyper::Response<hyper::Body> {
    fn from(err: ApiError) -> Self {
        match serde_json::to_vec(&err) {
            Ok(err_body) => hyper::Response::builder()
                .header(CONTENT_TYPE, "application/json")
                .status(err.status_code)
                .body(err_body.into())
                .expect("a valid response"),
            Err(_) => hyper::Response::builder()
                .header(CONTENT_TYPE, "application/json")
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("Internal error. Unable to return the error response.".into())
                .expect("a valid response"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    pub struct DummyError {
        pub debug_field: String,
        pub display_field: String,
    }

    impl std::error::Error for DummyError {}

    impl Display for DummyError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", &self.display_field)
        }
    }

    #[test]
    fn serialize_api_error() {
        let err = DummyError {
            debug_field: "debug error".into(),
            display_field: "display error".into(),
        };
        let api_err = ApiError {
            error: err.into(),
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            span_trace: None,
        };

        // Check serialized error response
        let api_err_serialized = serde_json::to_string(&api_err).unwrap();
        assert_eq!(api_err_serialized, r#"{"error":"display error"}"#);

        // The error report should use the Debug impl of the root cause
        let err_report = format!("{:?}", api_err.report());
        assert_eq!(
            err_report,
            r#"Report { cause: "DummyError { debug_field: \"debug error\", display_field: \"display error\" }", span_trace: None }"#
        );
    }
}
