use std::sync::Arc;

pub use covert_types::entity::EntityAlias;
pub use covert_types::methods::system::{
    AttachEntityAliasParams, AttachEntityAliasResponse, AttachEntityPolicyParams,
    AttachEntityPolicyResponse, CreateEntityParams, CreateEntityResponse, RemoveEntityAliasParams,
    RemoveEntityAliasResponse, RemoveEntityPolicyParams, RemoveEntityPolicyResponse,
};

use crate::base::BaseClient;

pub struct Client {
    client: Arc<BaseClient>,
}

impl Client {
    pub(crate) fn new(client: Arc<BaseClient>) -> Self {
        Self { client }
    }

    pub async fn create(
        &self,
        params: &CreateEntityParams,
    ) -> Result<CreateEntityResponse, String> {
        self.client.post("/sys/entity".into(), params).await
    }

    pub async fn attach_policies(
        &self,
        params: &AttachEntityPolicyParams,
    ) -> Result<AttachEntityPolicyResponse, String> {
        self.client.put("/sys/entity/policy".into(), params).await
    }

    pub async fn remove_policy(
        &self,
        name: &str,
        params: &RemoveEntityPolicyParams,
    ) -> Result<RemoveEntityPolicyResponse, String> {
        self.client
            .put(format!("/sys/entity/policy/{name}"), params)
            .await
    }

    pub async fn attach_alias(
        &self,
        params: &AttachEntityAliasParams,
    ) -> Result<AttachEntityAliasResponse, String> {
        self.client.put("/sys/entity/alias".into(), params).await
    }

    pub async fn remove_alias(
        &self,
        name: &str,
        params: &RemoveEntityAliasParams,
    ) -> Result<RemoveEntityAliasResponse, String> {
        self.client
            .put(format!("/sys/entity/alias/{name}"), params)
            .await
    }
}
