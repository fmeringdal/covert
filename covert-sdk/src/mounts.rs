use std::sync::Arc;

pub use covert_types::backend::{BackendCategory, BackendType};
use covert_types::methods::system::DisbaleMountResponse;
pub use covert_types::methods::system::{
    CreateMountParams, CreateMountResponse, MountsListResponse, UpdateMountParams,
    UpdateMountResponse,
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
        path: &str,
        params: &CreateMountParams,
    ) -> Result<CreateMountResponse, String> {
        self.client
            .post(format!("/sys/mounts/{path}"), params)
            .await
    }

    pub async fn update(
        &self,
        path: &str,
        params: &UpdateMountParams,
    ) -> Result<UpdateMountResponse, String> {
        self.client.put(format!("/sys/mounts/{path}"), params).await
    }

    pub async fn list(&self) -> Result<MountsListResponse, String> {
        self.client.get("/sys/mounts".into()).await
    }

    pub async fn remove(&self, path: &str) -> Result<DisbaleMountResponse, String> {
        self.client.delete(format!("/sys/mounts/{path}")).await
    }
}
