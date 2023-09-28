use std::sync::Arc;

use tokio::sync::broadcast;

use crate::{repos::Repos, Config, ExpirationManager, Router};

pub struct Context {
    pub config: Arc<Config>,
    pub repos: Repos,
    pub expiration_manager: Arc<ExpirationManager>,
    pub router: Arc<Router>,
    pub stop_tx: broadcast::Sender<()>,
}

impl Clone for Context {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            repos: self.repos.clone(),
            expiration_manager: Arc::clone(&self.expiration_manager),
            router: Arc::clone(&self.router),
            stop_tx: self.stop_tx.clone(),
        }
    }
}
