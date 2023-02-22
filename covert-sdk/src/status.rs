use std::sync::Arc;

pub use covert_types::methods::system::StatusResponse;

use crate::base::BaseClient;

pub struct Client {
    client: Arc<BaseClient>,
}

impl Client {
    pub(crate) fn new(client: Arc<BaseClient>) -> Self {
        Self { client }
    }

    pub async fn status(&self) -> Result<StatusResponse, String> {
        self.client.get("/sys/status".into()).await
    }
}
