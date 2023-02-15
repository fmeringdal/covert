use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};

use covert_framework::extract::{Extension, Json};
use covert_types::{
    entity::Entity,
    methods::system::{UnsealParams, UnsealResponse},
    policy::Policy,
    response::Response,
    token::Token,
};
use parking_lot::RwLock;

use crate::{
    error::{Error, ErrorType},
    migrations::{migrate, Migrations},
    repos::{token::TokenEntry, Repos},
    ExpirationManager, Router,
};

use super::mount::mount_route_entry;

pub struct UnsealProgress {
    pub provided_shares: Arc<RwLock<HashSet<String>>>,
    pub threshold: Arc<AtomicU8>,
    pub shares_count: Arc<AtomicU8>,
}

impl UnsealProgress {
    pub fn new() -> Self {
        Self {
            provided_shares: Arc::new(RwLock::new(HashSet::new())),
            threshold: Arc::new(AtomicU8::new(0)),
            shares_count: Arc::new(AtomicU8::new(0)),
        }
    }

    pub fn set_threshold(&self, threshold: u8) {
        self.threshold.store(threshold, Ordering::SeqCst);
    }

    pub fn threshold(&self) -> u8 {
        self.threshold.load(Ordering::SeqCst)
    }

    pub fn set_shares_count(&self, threshold: u8) {
        self.shares_count.store(threshold, Ordering::SeqCst);
    }

    pub fn shares_count(&self) -> u8 {
        self.shares_count.load(Ordering::SeqCst)
    }

    pub fn provide_shares(&self, shares: &[String]) {
        let mut keys = self.provided_shares.write();
        for share in shares {
            keys.insert(share.clone());
        }
    }

    pub fn shares(&self) -> HashSet<String> {
        let keys = self.provided_shares.read();
        keys.clone()
    }

    pub fn clear_shares(&self) {
        let mut keys = self.provided_shares.write();
        keys.drain();
    }
}

impl Clone for UnsealProgress {
    fn clone(&self) -> Self {
        Self {
            provided_shares: Arc::clone(&self.provided_shares),
            threshold: Arc::clone(&self.threshold),
            shares_count: Arc::clone(&self.shares_count),
        }
    }
}

pub async fn handle_unseal(
    Extension(repos): Extension<Repos>,
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(router): Extension<Arc<Router>>,
    Extension(unseal_progress): Extension<UnsealProgress>,
    Json(body): Json<UnsealParams>,
) -> Result<Response, Error> {
    let threshold = unseal_progress.threshold();
    unseal_progress.provide_shares(&body.shares);
    let shares = unseal_progress.shares();

    if usize::from(threshold) > shares.len() {
        // Return progress
        let resp = UnsealResponse::InProgress {
            threshold,
            key_shares_total: unseal_progress.shares_count(),
            key_shares_provided: shares.len(),
        };
        return Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into());
    }

    let master_key = construct_master_key(&shares, threshold).map_err(|err| {
        unseal_progress.clear_shares();
        err
    })?;
    unseal_progress.clear_shares();

    unseal(
        &repos,
        expiration_manager,
        router,
        unseal_progress,
        master_key,
    )
    .await?;

    let root_token = generate_root_token(&repos).await?;

    let resp = UnsealResponse::Complete { root_token };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

fn construct_master_key(key_shares: &HashSet<String>, threshold: u8) -> Result<String, Error> {
    let key_shares = key_shares
        .iter()
        .map(|s| {
            hex::decode(s)
                .map_err(|_| ErrorType::BadRequest("Malformed key shares".into()))
                .and_then(|share| {
                    sharks::Share::try_from(share.as_slice())
                        .map_err(|_| ErrorType::BadRequest("Malformed key shares".into()))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sharks = sharks::Sharks(threshold);
    let master_key = sharks
        .recover(key_shares.as_slice())
        .map_err(|_| ErrorType::MasterKeyRecovery)?;
    let master_key = String::from_utf8(master_key).map_err(|_| ErrorType::MasterKeyRecovery)?;
    Ok(master_key)
}

async fn unseal(
    repos: &Repos,
    expiration_manager: Arc<ExpirationManager>,
    router: Arc<Router>,
    unseal_progress: UnsealProgress,
    master_key: String,
) -> Result<(), Error> {
    repos.pool.unseal(master_key)?;

    // Run migrations
    migrate::<Migrations>(repos.pool.as_ref()).await?;

    let mounts = repos.mount.list().await?;
    if mounts.is_empty() {
        // TODO: Insert default KV backend
    }

    for mount in mounts {
        mount_route_entry(
            repos,
            Arc::clone(&expiration_manager),
            Arc::clone(&router),
            unseal_progress.clone(),
            mount.path,
            mount.id,
            mount.backend_type,
            mount.config,
        )
        .await?;
    }

    // Start expiration manager
    tokio::spawn(async move {
        if expiration_manager.start().await.is_err() {
            // TODO: stop the server
        }
    });

    Ok(())
}

pub async fn generate_root_token(repos: &Repos) -> Result<Token, Error> {
    // Generate root policy if not exist
    let policy = Policy::new("root".into(), vec![]);
    let _res = repos.policy.create(&policy).await;

    // Generate root entity if not exist
    let entity = Entity::new("root".into(), false);
    let _res = repos.entity.create(&entity).await;

    let te = TokenEntry::new_root();
    let token = te.id().clone();
    repos.token.create(&te).await?;

    Ok(token)
}
