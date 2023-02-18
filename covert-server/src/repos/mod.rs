use std::sync::Arc;

use covert_storage::EncryptedPool;
use sqlx::{Pool, Sqlite};

use self::{
    entity::EntityRepo, lease::LeaseRepo, mount::MountRepo, policy::PolicyRepo, seal::SealRepo,
    token::TokenRepo,
};

pub mod entity;
pub mod lease;
pub mod mount;
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
            seal: SealRepo::new(unecrypted_pool.clone()),
            pool,
            unecrypted_pool,
        }
    }
}
