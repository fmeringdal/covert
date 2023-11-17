use std::sync::Arc;

use chrono::Utc;
use covert_framework::extract::{Extension, Json};
use covert_types::{
    entity::Entity,
    methods::system::{UnsealParams, UnsealResponse},
    policy::{PathPolicy, Policy},
    request::Operation,
    response::Response,
    token::Token,
};
use tracing::error;
use uuid::Uuid;

use crate::{
    context::Context,
    error::{Error, ErrorType},
    recovery::{recover, replicate},
    repos::{namespace::Namespace, token::TokenEntry, Repos},
};

use super::mount::mount_route_entry;

pub async fn handle_unseal(
    Extension(ctx): Extension<Context>,
    Json(body): Json<UnsealParams>,
) -> Result<Response, Error> {
    let seal_config = ctx.repos.seal.get_config().await?.ok_or_else(|| {
        ErrorType::InternalError(anyhow::Error::msg(
            "Seal config was not found when unseal handler was called",
        ))
    })?;

    for key in body.shares {
        ctx.repos.seal.insert_key_share(key.as_bytes()).await?;
    }

    let Ok(shares) = ctx
        .repos
        .seal
        .get_key_shares()
        .await?
        .into_iter()
        .map(|k| String::from_utf8(k.key))
        .collect::<Result<Vec<_>, _>>()
    else {
        ctx.repos.seal.clear_key_shares().await?;
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
        ctx.repos.seal.clear_key_shares().await?;
        return Err(
            ErrorType::BadData("Unable to construct master key from key shares".into()).into(),
        );
    };
    // No longer needed so just clear them
    ctx.repos.seal.clear_key_shares().await?;

    unseal(&ctx, master_key).await?;

    let root_token = generate_root_token(&ctx.repos).await?;

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

async fn unseal(ctx: &Context, master_key: String) -> Result<(), Error> {
    // TODO: seal pool again if anything below fails

    // Try to recover encrypted storage if replication has not already started.
    // Replication could have already started if sealed and then unsealed again.
    // TODO: check if replication has already started
    if let Some(replication) = ctx.config.replication.as_ref() {
        // Setup replication
        recover(
            replication,
            &ctx.config.encrypted_storage_path(),
            &replication.encrypted_db_prefix(),
            Some(master_key.clone()),
        )
        .await
        .unwrap();
        match replicate(
            replication,
            Some(master_key.clone()),
            &ctx.config.encrypted_storage_path(),
            &replication.encrypted_db_prefix(),
            ctx.stop_tx.subscribe(),
        )
        .await
        {
            Ok(()) => {}
            Err(err) => {
                error!(?err, "Failed to setup replication");
            }
        }
    }

    ctx.repos.pool.unseal(master_key.clone())?;

    // Clear all shares now that master key is constructed
    ctx.repos.seal.clear_key_shares().await?;

    // Run migrations
    crate::migrations::migrate_ecrypted_db(ctx.repos.pool.as_ref()).await?;

    // Setup root namespace
    let ns = if let Some(ns) = ctx
        .repos
        .namespace
        .find_by_path(&["root".to_string()])
        .await?
    {
        ns
    } else {
        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ctx.repos.namespace.create(&ns).await?;
        ns
    };

    let mounts = ctx.repos.mount.list(&ns.id).await?;
    for mount in mounts {
        mount_route_entry(ctx, mount.id, mount.backend_type, &ns.id).await?;
    }

    // Start expiration manager
    let expiration_manager = Arc::clone(&ctx.expiration_manager);
    tokio::spawn(async move {
        if expiration_manager.start().await.is_err() {
            // TODO: stop the server
        }
    });

    Ok(())
}

pub async fn generate_root_token(repos: &Repos) -> Result<Token, Error> {
    let ns = repos
        .namespace
        .find_by_path(&["root".to_string()])
        .await?
        .ok_or_else(|| ErrorType::InternalError(anyhow::Error::msg("Missing root namespace")))?;

    // Generate root policy if not exist
    let policy = Policy::new(
        "root".into(),
        vec![PathPolicy {
            path: "*".to_string(),
            operations: vec![
                Operation::Read,
                Operation::Delete,
                Operation::Create,
                Operation::Update,
            ],
        }],
        ns.id.clone(),
    );
    let _res = repos.policy.create(&policy).await;

    // Generate root entity if not exist
    let entity = Entity::new("root".into(), ns.id.clone());
    let _res = repos.entity.create(&entity).await;

    // Attach root policy to root entity
    let _res = repos
        .entity
        .attach_policy(&entity.name, &policy.name, &ns.id)
        .await;

    let te = TokenEntry {
        id: Token::new(),
        entity_name: entity.name,
        expires_at: None,
        issued_at: Utc::now(),
        namespace_id: ns.id.clone(),
    };
    let token = te.id().clone();
    repos.token.create(&te).await?;

    let _res = repos.token.lookup_policies(&token).await;

    Ok(token)
}
