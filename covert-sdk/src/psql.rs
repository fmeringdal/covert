use std::{sync::Arc, time::Duration};

use covert_types::methods::psql::CreateRoleCredsParams;
pub use covert_types::methods::psql::{
    CreateRoleCredsResponse, CreateRoleParams, CreateRoleResponse, ReadConnectionResponse,
    SetConnectionParams, SetConnectionResponse,
};

use crate::{base::BaseClient, utils::get_mount_path};

pub struct Client {
    client: Arc<BaseClient>,
}

impl Client {
    pub(crate) fn new(client: Arc<BaseClient>) -> Self {
        Self { client }
    }

    pub async fn set_connection(
        &self,
        mount: &str,
        params: &SetConnectionParams,
    ) -> Result<SetConnectionResponse, String> {
        let path = get_mount_path(mount, "config/connection");
        self.client.post(path, params).await
    }

    pub async fn read_connection(&self, mount: &str) -> Result<ReadConnectionResponse, String> {
        let path = get_mount_path(mount, "config/connection");
        self.client.get(path).await
    }

    pub async fn create_credentials(
        &self,
        mount: &str,
        name: &str,
        ttl: Option<Duration>,
    ) -> Result<CreateRoleCredsResponse, String> {
        let path = get_mount_path(mount, &format!("creds/{name}"));
        self.client.put(path, &CreateRoleCredsParams { ttl }).await
    }

    pub async fn create_role(
        &self,
        mount: &str,
        name: &str,
        params: &CreateRoleParams,
    ) -> Result<CreateRoleResponse, String> {
        let path = get_mount_path(mount, &format!("roles/{name}"));
        self.client.post(path, params).await
    }
}
