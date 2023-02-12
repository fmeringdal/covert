use std::sync::Arc;

use covert_storage::BackendStoragePool;

use crate::store::{config::Repo as ConfigRepo, secrets::Repo as SecretsRepo};

#[derive(Debug)]
pub struct Repos {
    pub config: ConfigRepo,
    pub secrets: SecretsRepo,
}

#[derive(Debug)]
pub struct Context {
    pub repos: Arc<Repos>,
}

impl Context {
    pub fn new(storage: BackendStoragePool) -> Self {
        Self {
            repos: Arc::new(Repos {
                config: ConfigRepo::new(storage.clone()),
                secrets: SecretsRepo::new(storage),
            }),
        }
    }
}
