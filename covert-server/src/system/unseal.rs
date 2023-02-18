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
    let seal_config = repos.seal.get_config().await?.ok_or_else(|| {
        ErrorType::InternalError(anyhow::Error::msg(
            "Seal config was not found when unseal handler was called",
        ))
    })?;

    for key in body.shares {
        repos.seal.insert_key_share(key.as_bytes()).await?;
    }

    let Ok(shares) = repos
        .seal
        .get_key_shares()
        .await?
        .into_iter()
        .map(|k| String::from_utf8(k.key))
        .collect::<Result<Vec<_>, _>>() else {
            repos.seal.clear_key_shares().await?;
            return Err(ErrorType::BadData("Invalid share key found".into()).into());
        };

    if usize::from(seal_config.threshold) > shares.len() {
        // Return progress
        let resp = UnsealResponse::InProgress {
            threshold: seal_config.threshold,
            key_shares_total: seal_config.shares,
            key_shares_provided: shares.len(),
        };
        return Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into());
    }

    let Ok(master_key) = construct_master_key(&shares, seal_config.threshold) else {
        repos.seal.clear_key_shares().await?;
        return Err(ErrorType::BadData("Unable to construct master key from key shares".into()).into());
    };
    // No longer needed so just clear them
    repos.seal.clear_key_shares().await?;

    unseal(&repos, expiration_manager, router, master_key).await?;

    let root_token = generate_root_token(&repos).await?;

    let resp = UnsealResponse::Complete { root_token };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

fn construct_master_key(key_shares: &[String], threshold: u8) -> Result<String, Error> {
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
    master_key: String,
) -> Result<(), Error> {
    repos.pool.unseal(master_key)?;

    // Run migrations
    crate::migrations::migrate_ecrypted_db(repos.pool.as_ref()).await?;

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
