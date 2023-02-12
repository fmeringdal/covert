use std::sync::Arc;

pub use covert_types::methods::system::{
    CreatePolicyParams, CreatePolicyResponse, ListPolicyResponse, RemovePolicyResponse,
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
        params: &CreatePolicyParams,
    ) -> Result<CreatePolicyResponse, String> {
        self.client.post("/sys/policies".into(), params).await
    }

    pub async fn list(&self) -> Result<ListPolicyResponse, String> {
        self.client.get("/sys/policies".into()).await
    }

    pub async fn remove(&self, name: &str) -> Result<RemovePolicyResponse, String> {
        self.client.delete(format!("/sys/policies/{name}")).await
    }
}
