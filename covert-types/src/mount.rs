use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::backend::BackendType;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    #[serde(with = "humantime_serde")]
    pub default_lease_ttl: Duration,
    #[serde(with = "humantime_serde")]
    pub max_lease_ttl: Duration,
}

impl Default for MountConfig {
    fn default() -> Self {
        Self {
            default_lease_ttl: Duration::from_secs(60 * 30),
            max_lease_ttl: Duration::from_secs(60 * 60 * 4),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MountEntry {
    pub uuid: Uuid,
    pub path: String,
    pub config: MountConfig,
    pub backend_type: BackendType,
}
