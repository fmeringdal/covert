use std::sync::Arc;

use covert_framework::extract::{Extension, Json};
use covert_types::{
    entity::Entity,
    methods::system::{UnsealParams, UnsealResponse},
    policy::Policy,
    response::Response,
    token::Token,
};

use crate::{
    error::{Error, ErrorType},
    migrations::{migrate, Migrations},
    repos::{token::TokenEntry, Repos},
    ExpirationManager, Router,
};

use super::mount::mount_route_entry;

pub async fn handle_unseal(
    Extension(repos): Extension<Repos>,
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(router): Extension<Arc<Router>>,
    Json(body): Json<UnsealParams>,
) -> Result<Response, Error> {
    let threshold = u8::try_from(body.shares.len())
        .map_err(|_| ErrorType::BadRequest("Invalid number of shares".into()))?;

    let key_shares = body
        .shares
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

    unseal(&repos, expiration_manager, router, master_key).await?;

    let root_token = generate_root_token(&repos).await?;

    let resp = UnsealResponse { root_token };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

async fn unseal(
    repos: &Repos,
    expiration_manager: Arc<ExpirationManager>,
    router: Arc<Router>,
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
