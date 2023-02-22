use std::sync::Arc;

pub use covert_types::methods::system::{
    CreateNamespaceParams, CreateNamespaceResponse, DeleteNamespaceResponse, ListNamespaceResponse,
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
        params: &CreateNamespaceParams,
    ) -> Result<CreateNamespaceResponse, String> {
        self.client.post("/sys/namespaces".into(), params).await
    }

    pub async fn delete(&self, name: &str) -> Result<DeleteNamespaceResponse, String> {
        self.client.delete(format!("/sys/namespaces/{name}")).await
    }

    pub async fn list(&self) -> Result<ListNamespaceResponse, String> {
        self.client.get("/sys/namespaces".into()).await
    }
}
