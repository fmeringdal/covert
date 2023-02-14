pub mod kv;
pub mod psql;
pub mod system;
pub mod userpass;

use std::time::Duration;

use serde::{self, Deserialize, Serialize};

use crate::token::Token;

#[derive(Debug, Deserialize, Serialize)]
pub struct SecretLeaseResponse<T> {
    pub data: T,
    pub lease_id: String,
    #[serde(with = "humantime_serde")]
    pub ttl: std::time::Duration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: Token,
    pub lease_id: String,
    #[serde(with = "humantime_serde")]
    pub ttl: std::time::Duration,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RenewLeaseParams<T> {
    #[serde(with = "humantime_serde")]
    pub ttl: Duration,
    pub data: T,
}
