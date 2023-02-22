use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, ErrorType};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LeaseEntry {
    pub id: String,
    pub issued_mount_path: String,
    pub revoke_path: Option<String>,
    pub revoke_data: String,
    pub renew_path: Option<String>,
    pub renew_data: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_renewal_time: DateTime<Utc>,
    pub failed_revocation_attempts: u32,
    pub namespace_id: String,
}

impl LeaseEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new<T: Serialize>(
        issued_mount_path: String,
        revoke_path: Option<String>,
        revoke_data: &T,
        renew_path: Option<String>,
        renew_data: &T,
        now: DateTime<Utc>,
        ttl: Duration,
        namespace_id: String,
    ) -> Result<Self, Error> {
        let expires_at = now + ttl;
        let issued_at = now;
        let last_renewal_time = now;

        let lease_id = Uuid::new_v4().to_string();
        let revoke_data = serde_json::to_string(revoke_data)
            .map_err(|_| ErrorType::BadData("Unable to serialize revoke data".into()))?;
        let renew_data = serde_json::to_string(renew_data)
            .map_err(|_| ErrorType::BadData("Unable to serialize renew data".into()))?;

        Ok(LeaseEntry {
            id: lease_id,
            issued_mount_path,
            revoke_path,
            revoke_data,
            renew_path,
            renew_data,
            issued_at,
            expires_at,
            last_renewal_time,
            failed_revocation_attempts: 0,
            namespace_id,
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl PartialOrd for LeaseEntry {
    fn partial_cmp(&self, other: &LeaseEntry) -> Option<std::cmp::Ordering> {
        self.expires_at.partial_cmp(&other.expires_at)
    }
}

impl PartialEq for LeaseEntry {
    fn eq(&self, other: &LeaseEntry) -> bool {
        self.id == other.id
    }
}

impl Eq for LeaseEntry {}

impl Ord for LeaseEntry {
    fn cmp(&self, other: &LeaseEntry) -> std::cmp::Ordering {
        self.expires_at.cmp(&other.expires_at)
    }
}
