use std::sync::Arc;

use covert_storage::EncryptedPool;
use sqlx::{Pool, Sqlite};

use self::{
    entity::EntityRepo, lease::LeaseRepo, mount::MountRepo, namespace::NamespaceRepo,
    policy::PolicyRepo, seal::SealRepo, token::TokenRepo,
};

pub mod entity;
pub mod lease;
pub mod mount;
pub mod namespace;
pub mod policy;
pub mod seal;
pub mod token;

#[derive(Clone)]
pub struct Repos {
    pub entity: EntityRepo,
    pub lease: LeaseRepo,
    pub mount: MountRepo,
    pub policy: PolicyRepo,
    pub token: TokenRepo,
    pub namespace: NamespaceRepo,
    pub seal: SealRepo,
    pub pool: Arc<EncryptedPool>,
    pub unecrypted_pool: Pool<Sqlite>,
}

impl Repos {
    pub fn new(pool: Arc<EncryptedPool>, unecrypted_pool: Pool<Sqlite>) -> Self {
        Self {
            entity: EntityRepo::new(Arc::clone(&pool)),
            lease: LeaseRepo::new(Arc::clone(&pool)),
            mount: MountRepo::new(Arc::clone(&pool)),
            policy: PolicyRepo::new(Arc::clone(&pool)),
            token: TokenRepo::new(Arc::clone(&pool)),
            namespace: NamespaceRepo::new(Arc::clone(&pool)),
            seal: SealRepo::new(unecrypted_pool.clone()),
            pool,
            unecrypted_pool,
        }
    }

    pub async fn close(&self) {
        if let Ok(pool) = self.pool.pool() {
            pool.close().await;
        }
        self.unecrypted_pool.close().await;
    }
}
