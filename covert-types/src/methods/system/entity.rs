use serde::{Deserialize, Serialize};

use crate::entity::EntityAlias;

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateEntityParams {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateEntityResponse {
    pub entity: EntityWithPolicyAndAlias,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AttachEntityPolicyParams {
    pub name: String,
    pub policy_names: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AttachEntityPolicyResponse {
    pub entity: EntityWithPolicyAndAlias,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AttachEntityAliasParams {
    pub name: String,
    pub aliases: Vec<EntityAlias>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AttachEntityAliasResponse {
    pub entity: EntityWithPolicyAndAlias,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoveEntityPolicyParams {
    pub policy_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoveEntityPolicyResponse {
    pub entity: EntityWithPolicyAndAlias,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoveEntityAliasParams {
    pub alias: EntityAlias,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoveEntityAliasResponse {
    pub entity: EntityWithPolicyAndAlias,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListEntitiesResponse {
    pub entities: Vec<EntityWithPolicyAndAlias>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EntityWithPolicyAndAlias {
    pub name: String,
    pub policies: Vec<String>,
    pub aliases: Vec<EntityAlias>,
}
