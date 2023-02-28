use std::fmt::Display;

use covert_storage::{migrator::MigrationError, EncryptedPoolError};
use covert_types::{
    backend::BackendType,
    error::{ApiError, StatusCode},
};
use sqlx::{error::DatabaseError, sqlite::SqliteError};
use thiserror::Error;
use tracing_error::SpanTrace;

#[derive(Error, Debug)]
pub enum ErrorType {
    #[error("Internal error")]
    Storage(sqlx::Error),
    #[error("Internal error")]
    InternalError(anyhow::Error),
    #[error("Internal error")]
    BadData(String),
    #[error("Internal error")]
    BadResponseData(#[source] serde_json::Error),
    #[error("Internal error")]
    BadHttpResponseData(#[source] hyper::http::Error),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    NotFound(String),
    #[error("Failed to renew lease `{lease_id}`")]
    RenewLease {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
        lease_id: String,
    },
    #[error("Failed to revoke lease `{lease_id}`")]
    RevokeLease {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
        lease_id: String,
    },
    #[error("{0}")]
    BadRequest(String),
    #[error("Internal error")]
    Migration(#[from] MigrationError),
    #[error("Internal error")]
    BackendMigration {
        #[source]
        error: MigrationError,
        variant: BackendType,
    },
    #[error("Mount for path `{path}` was not found")]
    MountNotFound { path: String },
    #[error(
        "Unable to mount backend at `{path}` because of path overlap with mount at `{existing_path}`"
    )]
    MountPathConflict { path: String, existing_path: String },
    #[error("`{path}` is not a valid mount path. Error: `{error}`")]
    InvalidMountPath { path: String, error: String },
    #[error("`{variant}` cannot be mounted or removed")]
    InvalidMountType { variant: BackendType },
    #[error("Invalid initialize request")]
    InvalidInitializeParams,
    #[error("Unable to perform state transition. Error: {0}")]
    StateTransition(#[from] EncryptedPoolError),
    #[error("Unable to recover master key from the key shares")]
    MasterKeyRecovery,
    #[error("A resource with that identifier already exists")]
    UniqueConstraintViolation {
        #[source]
        error: sqlx::Error,
    },
    // TODO: better error message
    #[error("The resource update was not processable")]
    ForeignKeyViolation {
        #[source]
        error: sqlx::Error,
    },
    #[error("Only the root namespace can call seal")]
    SealInNonRootNamespace,
    #[error("Failed to restore backup")]
    Recovery {
        #[source]
        error: anyhow::Error,
    },
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
        #[allow(clippy::redundant_closure_for_method_calls)]
        if let Some(error_code) = err
            .as_database_error()
            .and_then(|db_err| db_err.try_downcast_ref::<SqliteError>())
        {
            if let Some(code) = error_code.code().map(|str| str.to_string()) {
                match &code[..] {
                    // FK constraint violation
                    "787" => {
                        return Self {
                            variant: ErrorType::ForeignKeyViolation { error: err },
                            span_trace: SpanTrace::capture(),
                        };
                    }
                    // UNIQUE constraint violation
                    "1555" => {
                        return Self {
                            variant: ErrorType::UniqueConstraintViolation { error: err },
                            span_trace: SpanTrace::capture(),
                        };
                    }
                    _ => {}
                }
            }
        }
        Self {
            variant: ErrorType::Storage(err),
            span_trace: SpanTrace::capture(),
        }
    }
}

impl From<MigrationError> for Error {
    fn from(err: MigrationError) -> Self {
        Self {
            variant: err.into(),
            span_trace: SpanTrace::capture(),
        }
    }
}

impl From<EncryptedPoolError> for Error {
    fn from(err: EncryptedPoolError) -> Self {
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
            ErrorType::Storage(_)
            | ErrorType::InternalError(_)
            | ErrorType::BadData(_)
            | ErrorType::BadResponseData(_)
            | ErrorType::BadHttpResponseData(_)
            | ErrorType::RenewLease { .. }
            | ErrorType::RevokeLease { .. }
            | ErrorType::Migration { .. }
            | ErrorType::StateTransition(_)
            | ErrorType::BackendMigration { .. }
            | ErrorType::Recovery { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorType::Unauthorized(_) | ErrorType::MasterKeyRecovery => StatusCode::UNAUTHORIZED,
            ErrorType::NotFound(_) | ErrorType::MountNotFound { .. } => StatusCode::NOT_FOUND,
            ErrorType::BadRequest(_)
            | ErrorType::InvalidMountPath { .. }
            | ErrorType::InvalidInitializeParams
            | ErrorType::InvalidMountType { .. } => StatusCode::BAD_REQUEST,
            ErrorType::MountPathConflict { .. } | ErrorType::UniqueConstraintViolation { .. } => {
                StatusCode::CONFLICT
            }
            ErrorType::ForeignKeyViolation { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorType::SealInNonRootNamespace => StatusCode::FORBIDDEN,
        };

        ApiError {
            error: err.variant.into(),
            status_code,
            span_trace: Some(err.span_trace),
        }
    }
}
