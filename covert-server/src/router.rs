use std::{collections::HashMap, sync::Arc};

use covert_framework::Backend;
use covert_types::{error::ApiError, mount::MountConfig, request::Request};
use futures::future::BoxFuture;
use tokio::sync::RwLock;
use tower::Service;
use uuid::Uuid;

use crate::{
    error::{Error, ErrorType},
    repos::{mount::MountRepo, namespace::Namespace},
    response::{ResponseContext, ResponseWithCtx},
    system::SYSTEM_MOUNT_PATH,
};

/// Router is used to do prefix based routing of a request to a logical backend
pub struct Router {
    // TODO: dashmap
    // mount id -> Backend
    backend_lookup: RwLock<HashMap<String, Arc<Backend>>>,
    mount_repo: MountRepo,
}

impl Router {
    #[must_use]
    pub fn new(mount_repo: MountRepo) -> Self {
        Router {
            backend_lookup: RwLock::new(HashMap::default()),
            mount_repo,
        }
    }

    #[tracing::instrument(
        skip(self, req),
        fields(
            path = req.path,
            operation = ?req.operation
        )
    )]
    pub async fn route(&self, mut req: Request) -> Result<ResponseWithCtx, ApiError> {
        let (backend, path, config) = match req.extensions.get::<Namespace>() {
            Some(_) if req.path.starts_with(SYSTEM_MOUNT_PATH) => {
                let backend = self
                    .get_system_mount()
                    .await
                    .ok_or_else(ApiError::internal_error)?;

                (
                    backend,
                    SYSTEM_MOUNT_PATH.to_string(),
                    MountConfig::default(),
                )
            }
            Some(ns) => {
                let mount = self
                    .mount_repo
                    .longest_prefix(&req.path, &ns.id)
                    .await?
                    .ok_or_else(|| {
                        Error::from(ErrorType::MountNotFound {
                            path: req.path.clone(),
                        })
                    })?;
                let backend_lookup = self.backend_lookup.read().await;
                let backend = backend_lookup
                    .get(&mount.id.to_string())
                    .map(Arc::clone)
                    .ok_or_else(ApiError::internal_error)?;

                (backend, mount.path, mount.config)
            }
            // Namespace can be null if not unsealed
            None => {
                // Only system backend can handle requests when not unsealed
                if !req.path.starts_with(SYSTEM_MOUNT_PATH) {
                    return Err(ApiError::unauthorized());
                }

                let backend = self
                    .get_system_mount()
                    .await
                    .ok_or_else(ApiError::internal_error)?;

                (
                    backend,
                    SYSTEM_MOUNT_PATH.to_string(),
                    MountConfig::default(),
                )
            }
        };

        req.advance_path(&path);
        req.extensions.insert(config.clone());

        let span = tracing::span!(
            tracing::Level::DEBUG,
            "backend_handle_request",
            backend_mount_path = path,
            backend_type = %backend.variant(),
        );
        let _enter = span.enter();

        backend.handle_request(req).await.map(|response| {
            let ctx = ResponseContext {
                backend_config: config,
                backend_mount_path: path,
            };
            ResponseWithCtx { response, ctx }
        })
    }

    pub async fn clear_mounts(&self) {
        let mut backend_lookup = self.backend_lookup.write().await;
        backend_lookup.clear();
    }

    // Mount is used to expose a logical backend at a given prefix, using a unique salt,
    // and the barrier view for that path.
    pub async fn mount(&self, mount_id: Uuid, backend: Arc<Backend>) {
        let mut backend_lookup = self.backend_lookup.write().await;
        backend_lookup.insert(mount_id.to_string(), backend);
    }

    pub async fn mount_system(&self, backend: Arc<Backend>) {
        let mut backend_lookup = self.backend_lookup.write().await;
        backend_lookup.insert("system".to_string(), backend);
    }

    pub async fn get_system_mount(&self) -> Option<Arc<Backend>> {
        let backend_lookup = self.backend_lookup.read().await;
        backend_lookup.get("system").map(Arc::clone)
    }

    pub async fn remove(&self, mount_id: Uuid) -> bool {
        let mut backend_lookup = self.backend_lookup.write().await;
        backend_lookup.remove(&mount_id.to_string()).is_some()
    }
}

#[derive(Clone)]
pub struct RouterService(Arc<Router>);

impl RouterService {
    pub fn new(router: Arc<Router>) -> Self {
        Self(router)
    }
}

impl Service<Request> for RouterService {
    type Response = ResponseWithCtx;

    type Error = ApiError;

    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let router = self.0.clone();
        Box::pin(async move { router.route(req).await })
    }
}
