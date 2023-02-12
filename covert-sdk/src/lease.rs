use std::sync::Arc;

pub use covert_types::methods::system::{
    ListLeasesResponse, LookupLeaseResponse, RenewLeaseResponse, RevokedLeaseResponse,
    RevokedLeasesResponse,
};

use crate::base::BaseClient;

pub struct Client {
    client: Arc<BaseClient>,
}

impl Client {
    pub(crate) fn new(client: Arc<BaseClient>) -> Self {
        Self { client }
    }

    pub async fn renew(&self, lease_id: &str) -> Result<RenewLeaseResponse, String> {
        self.client
            .put(format!("/sys/leases/renew/{lease_id}"), &())
            .await
    }

    pub async fn revoke(&self, lease_id: &str) -> Result<RevokedLeaseResponse, String> {
        self.client
            .put(format!("/sys/leases/revoke/{lease_id}"), &())
            .await
    }

    pub async fn lookup(&self, lease_id: &str) -> Result<LookupLeaseResponse, String> {
        self.client
            .get(format!("/sys/leases/lookup/{lease_id}"))
            .await
    }

    pub async fn revoke_by_mount(&self, prefix: &str) -> Result<RevokedLeasesResponse, String> {
        self.client
            .put(format!("/sys/leases/revoke-mount/{prefix}"), &())
            .await
    }

    pub async fn list_by_mount(&self, prefix: &str) -> Result<ListLeasesResponse, String> {
        self.client
            .get(format!("/sys/leases/lookup-mount/{prefix}"))
            .await
    }
}
