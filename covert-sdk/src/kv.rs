use std::sync::Arc;

use covert_types::methods::kv::CreateSecretResponse;
pub use covert_types::methods::kv::{
    CreateSecretParams, HardDeleteSecretParams, HardDeleteSecretResponse, ReadConfigResponse,
    ReadSecretResponse, RecoverSecretParams, RecoverSecretResponse, SetConfigParams,
    SetConfigResponse, SoftDeleteSecretParams, SoftDeleteSecretResponse,
};

use crate::{base::BaseClient, utils::get_mount_path};

pub struct Client {
    config: Arc<BaseClient>,
}

impl Client {
    pub(crate) fn new(config: Arc<BaseClient>) -> Self {
        Self { config }
    }

    pub async fn create(
        &self,
        mount: &str,
        key: &str,
        params: &CreateSecretParams,
    ) -> Result<CreateSecretResponse, String> {
        let path = get_mount_path(mount, &format!("data/{key}"));
        self.config.post(path, params).await
    }

    pub async fn read(
        &self,
        mount: &str,
        key: &str,
        version: Option<u32>,
    ) -> Result<ReadSecretResponse, String> {
        let mut path = get_mount_path(mount, &format!("data/{key}"));
        if let Some(version) = version {
            path = format!("{path}?version={version}");
        }
        self.config.get(path).await
    }

    pub async fn set_config(
        &self,
        mount: &str,
        params: &SetConfigParams,
    ) -> Result<SetConfigResponse, String> {
        let path = get_mount_path(mount, "config");
        self.config.post(path, params).await
    }

    pub async fn read_config(&self, mount: &str) -> Result<ReadConfigResponse, String> {
        let path = get_mount_path(mount, "config");
        self.config.get(path).await
    }

    pub async fn delete(
        &self,
        mount: &str,
        key: &str,
        params: &SoftDeleteSecretParams,
    ) -> Result<SoftDeleteSecretResponse, String> {
        let path = get_mount_path(mount, &format!("delete/{key}"));
        self.config.post(path, params).await
    }

    pub async fn recover(
        &self,
        mount: &str,
        key: &str,
        params: &RecoverSecretParams,
    ) -> Result<RecoverSecretResponse, String> {
        let path = get_mount_path(mount, &format!("undelete/{key}"));
        self.config.post(path, params).await
    }

    pub async fn hard_delete(
        &self,
        mount: &str,
        key: &str,
        params: &HardDeleteSecretParams,
    ) -> Result<HardDeleteSecretResponse, String> {
        let path = get_mount_path(mount, &format!("destroy/{key}"));
        self.config.post(path, params).await
    }
}
