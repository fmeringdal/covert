use std::{process::Child, sync::Arc};

use tokio::sync::RwLock;
use tracing::error;

use crate::{repos::Repos, Config, ExpirationManager, Router};

pub struct Context {
    pub config: Arc<Config>,
    pub repos: Repos,
    pub child_processes: ChildProcesses,
    pub expiration_manager: Arc<ExpirationManager>,
    pub router: Arc<Router>,
}

impl Clone for Context {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            repos: self.repos.clone(),
            child_processes: self.child_processes.clone(),
            expiration_manager: Arc::clone(&self.expiration_manager),
            router: Arc::clone(&self.router),
        }
    }
}

pub struct ChildProcesses {
    encrypted_storage_replication: Arc<RwLock<Option<Child>>>,
    seal_storage_replication: Arc<RwLock<Option<Child>>>,
}

impl Clone for ChildProcesses {
    fn clone(&self) -> Self {
        Self {
            encrypted_storage_replication: Arc::clone(&self.encrypted_storage_replication),
            seal_storage_replication: Arc::clone(&self.seal_storage_replication),
        }
    }
}

impl Default for ChildProcesses {
    fn default() -> Self {
        Self {
            encrypted_storage_replication: Arc::new(RwLock::new(None)),
            seal_storage_replication: Arc::new(RwLock::new(None)),
        }
    }
}

impl ChildProcesses {
    pub async fn encrypted_storage_replication_started(&self) -> bool {
        self.encrypted_storage_replication.read().await.is_some()
    }

    pub async fn set_encrypted_storage_replication(&self, child: Child) {
        let mut l = self.encrypted_storage_replication.write().await;
        *l = Some(child);
    }

    pub async fn set_seal_storage_replication(&self, child: Child) {
        let mut l = self.seal_storage_replication.write().await;
        *l = Some(child);
    }

    pub async fn kill_all(&self) {
        if let Some(mut c) = self.encrypted_storage_replication.write().await.take() {
            if c.kill().is_err() {
                error!("Failed to kill seal storage replication process");
            }
            let _ = c.wait();
        }
        if let Some(mut c) = self.seal_storage_replication.write().await.take() {
            if c.kill().is_err() {
                error!("Failed to kill seal storage replication process");
            }
            let _ = c.wait();
        }
    }
}
