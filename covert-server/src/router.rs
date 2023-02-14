use std::sync::Arc;

use covert_framework::Backend;
use covert_types::{error::ApiError, mount::MountConfig, request::Request};
use futures::future::BoxFuture;
use tokio::sync::RwLock;
use tower::Service;
use uuid::Uuid;

use crate::{
    error::{Error, ErrorType},
    helpers::trie::{NodeRef, Trie},
    response::{ResponseContext, ResponseWithCtx},
};

/// Router is used to do prefix based routing of a request to a logical backend
#[derive(Debug)]
pub struct Router {
    root: RwLock<Trie<Arc<RouteEntry>>>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    #[must_use]
    pub fn new() -> Self {
        Router {
            root: RwLock::new(Trie::default()),
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
        // Find the mount point
        let trie = self.root.read().await;
        let (re, mount) = trie
            .longest_prefix(&req.path)
            .map(|node| (Arc::clone(node.value), node.prefix.to_string()))
            .ok_or_else(|| {
                Error::from(ErrorType::MountNotFound {
                    path: req.path.clone(),
                })
            })?;
        drop(trie);

        req.advance_path(&mount);
        req.extensions.insert(re.config_cloned());

        let span = tracing::span!(
            tracing::Level::DEBUG,
            "backend_handle_request",
            backend_mount_path = mount,
            backend_type = %re.backend.variant(),
        );
        let _enter = span.enter();

        re.backend.handle_request(req).await.map(|response| {
            let ctx = ResponseContext {
                backend_config: re.config_cloned(),
                backend_mount_path: mount,
                backend_id: re.id,
            };
            ResponseWithCtx { response, ctx }
        })
    }

    pub async fn mounts(&self) -> Vec<TrieMount<RouteEntry>> {
        let trie = self.root.read().await;
        let mut mounts: Vec<NodeRef<'_, Arc<RouteEntry>>> = trie.mounts();
        mounts.sort_by_key(|node| node.prefix);
        mounts
            .into_iter()
            .map(|node| TrieMount {
                value: Arc::clone(node.value),
                path: node.prefix.to_string(),
            })
            .collect()
    }

    pub async fn clear_mounts(&self) {
        let mut trie = self.root.write().await;
        trie.clear();
    }

    // Mount is used to expose a logical backend at a given prefix, using a unique salt,
    // and the barrier view for that path.
    pub async fn mount(&self, re: RouteEntry) -> Result<(), Error> {
        let mut trie = self.root.write().await;
        // Check if this is a nested mount
        if let Some(existing) = trie.longest_prefix(&re.path) {
            return Err(ErrorType::MountPathConflict {
                path: re.path,
                existing_path: existing.prefix.to_string(),
            }
            .into());
        }
        let re = Arc::new(re);
        let path = re.path.clone();

        // Create a route entry
        trie.insert(&path, re);

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub async fn update_mount(&self, path: &str, config: MountConfig) -> Result<(), Error> {
        let mut trie = self.root.write().await;

        let re = trie
            .get(path)
            .map(Arc::clone)
            .ok_or_else(|| ErrorType::MountNotFound {
                path: path.to_string(),
            })?;
        let mut old_config = re.config.write();
        *old_config = config;
        drop(old_config);
        trie.insert(path, re);

        Ok(())
    }

    pub async fn remove(&self, path: &str) -> bool {
        let mut trie = self.root.write().await;
        trie.remove(path)
    }

    pub async fn get(&self, path: &str) -> Option<Arc<RouteEntry>> {
        let trie = self.root.read().await;
        trie.get(path).map(Arc::clone)
    }
}

#[derive(Debug)]
pub struct TrieMount<T> {
    pub path: String,
    pub value: Arc<T>,
}

#[derive(Debug)]
pub struct RouteEntry {
    id: Uuid,
    path: String,
    backend: Arc<Backend>,
    config: Arc<parking_lot::RwLock<MountConfig>>,
}

impl RouteEntry {
    pub fn new(
        id: Uuid,
        path: String,
        backend: Arc<Backend>,
        config: MountConfig,
    ) -> Result<Self, Error> {
        if path.is_empty() {
            return Err(ErrorType::InvalidMountPath {
                path,
                error: "Mount path cannot be empty".into(),
            }
            .into());
        }
        if path.starts_with('/') {
            return Err(ErrorType::InvalidMountPath {
                path,
                error: "Mount path cannot start with a '/'".into(),
            }
            .into());
        }
        if !path.ends_with('/') {
            return Err(ErrorType::InvalidMountPath {
                path,
                error: "Mount path should end with a '/'".into(),
            }
            .into());
        }

        Ok(Self {
            id,
            path,
            backend,
            config: Arc::new(parking_lot::RwLock::new(config)),
        })
    }

    pub fn backend(&self) -> &Backend {
        self.backend.as_ref()
    }

    pub fn config_cloned(&self) -> MountConfig {
        self.config.read().clone()
    }

    pub fn id(&self) -> Uuid {
        self.id
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
