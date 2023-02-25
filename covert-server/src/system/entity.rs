use covert_framework::extract::{Extension, Json, Path};
use covert_types::{
    entity::Entity,
    methods::system::{
        AttachEntityAliasParams, AttachEntityAliasResponse, AttachEntityPolicyParams,
        AttachEntityPolicyResponse, CreateEntityParams, CreateEntityResponse,
        EntityWithPolicyAndAlias, ListEntitiesResponse, RemoveEntityAliasParams,
        RemoveEntityAliasResponse, RemoveEntityPolicyParams, RemoveEntityPolicyResponse,
    },
    response::Response,
};

use crate::{
    error::{Error, ErrorType},
    repos::{namespace::Namespace, Repos},
};

#[tracing::instrument(skip(repos))]
pub async fn handle_entity_create(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Json(params): Json<CreateEntityParams>,
) -> Result<Response, Error> {
    let entity = Entity {
        name: params.name,
        namespace_id: ns.id.clone(),
    };
    repos.entity.create(&entity).await?;

    let resp = CreateEntityResponse {
        entity: EntityWithPolicyAndAlias {
            name: entity.name,
            policies: vec![],
            aliases: vec![],
        },
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(repos))]
pub async fn handle_attach_entity_policy(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Json(params): Json<AttachEntityPolicyParams>,
) -> Result<Response, Error> {
    let entity_policies = repos
        .policy
        .batch_lookup(&params.policy_names, &ns.id)
        .await;
    if entity_policies.len() != params.policy_names.len() {
        let entity_policies_names = entity_policies
            .into_iter()
            .map(|ep| ep.name)
            .collect::<Vec<_>>();
        let not_found_policies = params
            .policy_names
            .clone()
            .into_iter()
            .filter(|p| !entity_policies_names.contains(p))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ErrorType::NotFound(format!(
            "Could not find policies: `{not_found_policies}`"
        ))
        .into());
    }

    let mut attached_policies = vec![];
    for policy in &params.policy_names {
        if let Err(error) = repos
            .entity
            .attach_policy(&params.name, policy, &ns.id)
            .await
        {
            tracing::error!(
                ?error,
                policy,
                entity = params.name,
                "Unable to attach policy to entity",
            );
            continue;
        }
        attached_policies.push(policy.to_string());
    }

    let entity = lookup_entity(&repos, &params.name, &ns.id).await?;
    let resp = AttachEntityPolicyResponse { entity };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(repos))]
pub async fn handle_attach_entity_alias(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Json(params): Json<AttachEntityAliasParams>,
) -> Result<Response, Error> {
    let mut attached_aliases = vec![];
    for alias in &params.aliases {
        if let Err(error) = repos.entity.attach_alias(&params.name, alias, &ns.id).await {
            tracing::error!(
                ?error,
                ?alias,
                entity = params.name,
                "Unable to attach alias to entity",
            );
            continue;
        }
        attached_aliases.push(alias.clone());
    }

    let entity = lookup_entity(&repos, &params.name, &ns.id).await?;
    let resp = AttachEntityAliasResponse { entity };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(repos))]
pub async fn handle_remove_entity_policy(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Path(name): Path<String>,
    Json(params): Json<RemoveEntityPolicyParams>,
) -> Result<Response, Error> {
    if !repos
        .entity
        .remove_policy(&name, &params.policy_name, &ns.id)
        .await?
    {
        return Err(ErrorType::NotFound(format!(
            "Did not find a policy `{}` attached to entity `{name}`",
            params.policy_name
        ))
        .into());
    }

    let entity = lookup_entity(&repos, &name, &ns.id).await?;
    let resp = RemoveEntityPolicyResponse { entity };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(repos))]
pub async fn handle_remove_entity_alias(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Path(name): Path<String>,
    Json(params): Json<RemoveEntityAliasParams>,
) -> Result<Response, Error> {
    if !repos
        .entity
        .remove_alias(&name, &params.alias, &ns.id)
        .await?
    {
        return Err(ErrorType::NotFound(format!(
            "Did not find a alias `{}` for mount `{}` attached to entity `{name}`",
            params.alias.name, params.alias.mount_path
        ))
        .into());
    }

    let entity = lookup_entity(&repos, &name, &ns.id).await?;
    let resp = RemoveEntityAliasResponse { entity };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(repos))]
pub async fn handle_list_entities(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
) -> Result<Response, Error> {
    let entities = repos.entity.list(&ns.id).await?;

    let resp = ListEntitiesResponse {
        entities: entities
            .into_iter()
            .map(|e| EntityWithPolicyAndAlias {
                name: e.name,
                policies: e.policies,
                aliases: e.aliases,
            })
            .collect(),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

async fn lookup_entity(
    repos: &Repos,
    name: &str,
    namespace_id: &str,
) -> Result<EntityWithPolicyAndAlias, Error> {
    let entity = repos
        .entity
        .lookup(name, namespace_id)
        .await?
        .ok_or_else(|| {
            ErrorType::InternalError(anyhow::Error::msg(
                "Failed to lookup entity after modification",
            ))
        })?;
    Ok(EntityWithPolicyAndAlias {
        name: entity.name,
        policies: entity.policies,
        aliases: entity.aliases,
    })
}
