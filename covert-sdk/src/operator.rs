use std::sync::Arc;

pub use covert_types::methods::system::{
    InitializeParams, InitializeResponse, SealResponse, UnsealParams, UnsealResponse,
};

use crate::base::BaseClient;

pub struct Client {
    client: Arc<BaseClient>,
}

impl Client {
    pub(crate) fn new(client: Arc<BaseClient>) -> Self {
        Self { client }
    }

    pub async fn initialize(
        &self,
        params: &InitializeParams,
    ) -> Result<InitializeResponse, String> {
        self.client.post("/sys/init".into(), params).await
    }

    pub async fn unseal(&self, params: &UnsealParams) -> Result<UnsealResponse, String> {
        self.client.post("/sys/unseal".into(), params).await
    }

    pub async fn seal(&self) -> Result<SealResponse, String> {
        self.client.post("/sys/seal".into(), &()).await
    }
}
