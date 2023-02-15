use std::sync::Arc;

pub use covert_types::methods::{
    userpass::{
        CreateUserParams, CreateUserResponse, ListUsersResponse, LoginParams, RemoveUserResponse,
        UpdateUserPasswordParams, UpdateUserPasswordResponse,
    },
    AuthResponse,
};

use crate::{base::BaseClient, utils::get_mount_path};

pub struct Client {
    client: Arc<BaseClient>,
}

impl Client {
    pub(crate) fn new(client: Arc<BaseClient>) -> Self {
        Self { client }
    }

    pub async fn create(
        &self,
        mount: &str,
        params: &CreateUserParams,
    ) -> Result<CreateUserResponse, String> {
        let path = get_mount_path(mount, "users");
        self.client.post(path, params).await
    }

    pub async fn list(&self, mount: &str) -> Result<ListUsersResponse, String> {
        let path = get_mount_path(mount, "users");
        self.client.get(path).await
    }

    pub async fn login(&self, mount: &str, params: &LoginParams) -> Result<AuthResponse, String> {
        let path = get_mount_path(mount, "login");
        self.client.put(path, params).await
    }

    pub async fn remove(&self, mount: &str, username: &str) -> Result<RemoveUserResponse, String> {
        let path = get_mount_path(mount, &format!("users/{username}"));
        self.client.delete(path).await
    }

    pub async fn update_password(
        &self,
        mount: &str,
        username: &str,
        params: &UpdateUserPasswordParams,
    ) -> Result<UpdateUserPasswordResponse, String> {
        let path = get_mount_path(mount, &format!("users/{username}/password"));
        self.client.put(path, params).await
    }
}
