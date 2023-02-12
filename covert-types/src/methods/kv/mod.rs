use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct SetConfigParams {
    pub max_versions: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SetConfigResponse {
    pub max_versions: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReadConfigResponse {
    pub max_versions: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct CreateSecretParams {
    pub data: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateSecretResponse {
    pub version: u32,
    pub min_version: u32,
    pub max_version: u32,
    pub created_time: DateTime<Utc>,
    pub deleted: bool,
    pub destroyed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReadSecretQuery {
    #[serde(default)]
    pub version: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReadSecretResponse {
    pub data: Option<HashMap<String, String>>,
    pub metadata: CreateSecretResponse,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct HardDeleteSecretParams {
    pub versions: Vec<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HardDeleteSecretResponse {
    pub not_deleted: Vec<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct SoftDeleteSecretParams {
    pub versions: Vec<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SoftDeleteSecretResponse {
    pub not_deleted: Vec<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct RecoverSecretParams {
    pub versions: Vec<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RecoverSecretResponse {
    pub not_recovered: Vec<u32>,
}
