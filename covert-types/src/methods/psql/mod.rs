use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::psql::{ConnectionConfig, RoleCredentials};

use super::SecretLeaseResponse;

#[derive(Debug, Deserialize, Serialize)]
pub struct SetConnectionParams {
    pub connection_url: String,
    #[serde(default = "default_as_true")]
    pub verify_connection: bool,
    pub max_open_connections: Option<u32>,
}

fn default_as_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SetConnectionResponse {
    pub connection: ConnectionConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReadConnectionResponse {
    pub connection: Option<ConnectionConfig>,
}

pub type CreateRoleCredsResponse = SecretLeaseResponse<RoleCredentials>;

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateRoleParams {
    pub sql: String,
    pub revocation_sql: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateRoleResponse {
    pub sql: String,
    pub revocation_sql: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RenewLeaseParams {
    #[serde(with = "humantime_serde")]
    pub ttl: Option<Duration>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RenewLeaseResponse {
    #[serde(with = "humantime_serde")]
    pub ttl: Duration,
}
