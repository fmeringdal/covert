use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, sqlx::FromRow, PartialEq, Eq)]
pub struct ConnectionConfig {
    pub connection_url: String,
    pub max_open_connections: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RoleCredentials {
    pub username: String,
    pub password: String,
}
