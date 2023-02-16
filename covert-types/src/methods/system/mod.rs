mod entity;
mod policy;

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    backend::{BackendCategory, BackendType},
    mount::MountConfig,
    state::StorageState,
    token::Token,
};
pub use entity::*;
pub use policy::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeParams {
    pub shares: u8,
    pub threshold: u8,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InitializeResponse {
    NewKeyShares(InitializedKeyShares),
    ExistingKey(InitializedWithExistingKey),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializedKeyShares {
    pub shares: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializedWithExistingKey {
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnsealParams {
    pub shares: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnsealResponse {
    pub root_token: Token,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SealResponse {
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub state: StorageState,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateMountParams {
    #[serde(rename = "type")]
    pub variant: BackendType,
    #[serde(default)]
    pub config: MountConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateMountResponse {
    #[serde(rename = "type")]
    pub variant: BackendType,
    #[serde(default)]
    pub config: MountConfig,
    pub id: Uuid,
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateMountParams {
    pub config: MountConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateMountResponse {
    #[serde(rename = "type")]
    pub variant: BackendType,
    #[serde(default)]
    pub config: MountConfig,
    pub id: Uuid,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MountsListResponse {
    pub auth: Vec<MountsListItemResponse>,
    pub secret: Vec<MountsListItemResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DisableMountResponse {
    pub mount: MountsListItemResponse,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MountsListItemResponse {
    pub id: Uuid,
    pub path: String,
    pub category: BackendCategory,
    #[serde(rename = "type")]
    pub variant: BackendType,
    pub config: MountConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LeaseEntry {
    pub id: String,
    pub issued_mount_path: String,
    pub issue_time: String,
    pub expire_time: String,
    pub last_renewal_time: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RevokedLeasesResponse {
    pub leases: Vec<LeaseEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RevokedLeaseResponse {
    pub lease: LeaseEntry,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LookupLeaseResponse {
    pub lease: LeaseEntry,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListLeasesResponse {
    pub leases: Vec<LeaseEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RenewLeaseParams {
    #[serde(with = "humantime_serde")]
    pub ttl: Option<Duration>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RenewLeaseResponse {
    pub lease: LeaseEntry,
}
