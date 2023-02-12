use std::sync::Arc;

use covert_framework::extract::{Extension, Json, Path};
use covert_types::{
    entity::Entity,
    methods::system::{
        AttachEntityAliasParams, AttachEntityAliasResponse, AttachEntityPolicyParams,
        AttachEntityPolicyResponse, CreateEntityParams, CreateEntityResponse,
        RemoveEntityAliasParams, RemoveEntityAliasResponse, RemoveEntityPolicyParams,
        RemoveEntityPolicyResponse,
    },
    policy::Policy,
    response::Response,
};

use crate::{
    error::{Error, ErrorType},
    layer::auth_service::Permissions,
    store::{identity_store::IdentityStore, policy_store::PolicyStore},
};

#[tracing::instrument(skip(identity_store))]
pub async fn handle_entity_create(
    Extension(identity_store): Extension<Arc<IdentityStore>>,
    Json(params): Json<CreateEntityParams>,
) -> Result<Response, Error> {
    let entity = Entity {
        name: params.name,
        disabled: false,
    };
    identity_store.create(&entity).await?;

    let resp = CreateEntityResponse { entity };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(identity_store, policy_store))]
pub async fn handle_attach_entity_policy(
    Extension(identity_store): Extension<Arc<IdentityStore>>,
    Extension(policy_store): Extension<Arc<PolicyStore>>,
    Extension(permissions): Extension<Permissions>,
    Json(params): Json<AttachEntityPolicyParams>,
) -> Result<Response, Error> {
    // TODO: new endpoint for assigning policies to entity
    let entity_policies = policy_store.batch_lookup(&params.policy_names).await;
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

    match permissions {
        Permissions::Root => (),
        Permissions::Authenticated(policies) => {
            if !Policy::batch_is_authorized(&policies, &entity_policies) {
                return Err(ErrorType::Unauthorized(
                    "User does not have permission to assign these policies to entity".into(),
                )
                .into());
            }
        }
        Permissions::Unauthenticated => {
            return Err(ErrorType::Unauthorized(
                "User needs to be authenticated to assign policies".into(),
            )
            .into())
        }
    }

    let mut attached_policies = vec![];
    for policy in &params.policy_names {
        if let Err(error) = identity_store.attach_policy(&params.name, policy).await {
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

    let resp = AttachEntityPolicyResponse {
        policy_names: attached_policies,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(identity_store))]
pub async fn handle_attach_entity_alias(
    Extension(identity_store): Extension<Arc<IdentityStore>>,
    Json(params): Json<AttachEntityAliasParams>,
) -> Result<Response, Error> {
    let mut attached_aliases = vec![];
    for alias in &params.aliases {
        if let Err(error) = identity_store.attach_alias(&params.name, alias).await {
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

    let resp = AttachEntityAliasResponse {
        aliases: attached_aliases,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(identity_store))]
pub async fn handle_remove_entity_policy(
    Extension(identity_store): Extension<Arc<IdentityStore>>,
    Path(name): Path<String>,
    Json(params): Json<RemoveEntityPolicyParams>,
) -> Result<Response, Error> {
    if !identity_store
        .remove_policy(&name, &params.policy_name)
        .await?
    {
        return Err(ErrorType::NotFound(format!(
            "Did not find a policy `{}` attached to entity `{name}`",
            params.policy_name
        ))
        .into());
    }

    let resp = RemoveEntityPolicyResponse {
        policy_name: params.policy_name,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(identity_store))]
pub async fn handle_remove_entity_alias(
    Extension(identity_store): Extension<Arc<IdentityStore>>,
    Path(name): Path<String>,
    Json(params): Json<RemoveEntityAliasParams>,
) -> Result<Response, Error> {
    if !identity_store.remove_alias(&name, &params.alias).await? {
        return Err(ErrorType::NotFound(format!(
            "Did not find a alias `{}` for mount `{}` attached to entity `{name}`",
            params.alias.name, params.alias.mount_path
        ))
        .into());
    }

    let resp = RemoveEntityAliasResponse {
        alias: params.alias,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
