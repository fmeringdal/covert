#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![deny(clippy::pedantic)]
#![deny(clippy::get_unwrap)]
#![allow(clippy::module_name_repetitions)]

pub mod extract;
mod handler;
mod method_router;
mod router;
mod sync_service;

use std::sync::Arc;

use covert_storage::{
    migrator::{migrate, MigrationError, MigrationScript},
    EncryptedPool,
};
use tower::ServiceExt;

pub use method_router::*;
pub use router::Router;
pub use sync_service::SyncService;

use covert_types::{
    backend::{BackendCategory, BackendType},
    error::ApiError,
    request::Request,
    response::Response,
};

#[derive(Debug)]
pub struct Backend {
    pub handler: SyncService<Request, Response>,
    pub category: BackendCategory,
    pub variant: BackendType,
    pub migrations: Vec<MigrationScript>,
}

impl Backend {
    /// Call the backend with a [`Request`].
    ///
    /// # Errors
    ///
    /// Returns error if the backend fails to handle the [`Request`].
    pub async fn handle_request(&self, req: Request) -> Result<Response, ApiError> {
        let handler = self.handler.clone();
        handler.oneshot(req).await
    }

    #[must_use]
    pub fn category(&self) -> BackendCategory {
        self.category
    }

    #[must_use]
    pub fn variant(&self) -> BackendType {
        self.variant
    }

    /// Run migration for backend.
    ///
    /// # Errors
    ///
    /// Returns error if migration fails.
    #[tracing::instrument(skip(self, pool))]
    pub async fn migrate(
        &self,
        pool: Arc<EncryptedPool>,
        mount_id: &str,
        prefix: &str,
    ) -> Result<(), MigrationError> {
        migrate(pool.as_ref(), &self.migrations, mount_id, prefix).await
    }
}
