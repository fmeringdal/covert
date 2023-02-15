use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateUserParams {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateUserResponse {
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListUsersResponse {
    pub users: Vec<UserListItem>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserListItem {
    pub username: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateUserPasswordParams {
    pub password: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateUserPasswordResponse {
    pub username: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoveUserResponse {
    pub username: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LoginParams {
    pub username: String,
    pub password: String,
}
