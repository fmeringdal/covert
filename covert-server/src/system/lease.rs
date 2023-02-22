use std::sync::Arc;

use covert_framework::extract::{Extension, Json, Path};
use covert_types::{
    methods::system::{
        LeaseEntry as LeaseEntryDTO, ListLeasesResponse, LookupLeaseResponse, RenewLeaseParams,
        RenewLeaseResponse, RevokedLeaseResponse, RevokedLeasesResponse,
    },
    response::Response,
};

use crate::{
    error::{Error, ErrorType},
    expiration_manager::LeaseEntry,
    repos::namespace::Namespace,
    ExpirationManager,
};

impl From<&LeaseEntry> for LeaseEntryDTO {
    fn from(le: &LeaseEntry) -> Self {
        Self {
            id: le.id.clone(),
            issued_mount_path: le.issued_mount_path.clone(),
            issue_time: le.issued_at.to_rfc3339(),
            expire_time: le.expires_at.to_rfc3339(),
            last_renewal_time: le.expires_at.to_rfc3339(),
        }
    }
}

pub async fn handle_lease_revocation_by_mount(
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(ns): Extension<Namespace>,
    Path(prefix): Path<String>,
) -> Result<Response, Error> {
    let revoked_leases = expiration_manager
        .revoke_leases_by_mount_prefix(&prefix, &ns.id)
        .await?;
    let resp = RevokedLeasesResponse {
        leases: revoked_leases.iter().map(LeaseEntryDTO::from).collect(),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_lease_revocation(
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(ns): Extension<Namespace>,
    Path(lease_id): Path<String>,
) -> Result<Response, Error> {
    let revoked_lease = expiration_manager
        .revoke_lease_entry_by_id(&lease_id, &ns.id)
        .await?;
    let resp = RevokedLeaseResponse {
        lease: LeaseEntryDTO::from(&revoked_lease),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_list_leases(
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(ns): Extension<Namespace>,
    Path(prefix): Path<String>,
) -> Result<Response, Error> {
    let leases = expiration_manager
        .list_by_mount_prefix(&prefix, &ns.id)
        .await?;
    let resp = ListLeasesResponse {
        leases: leases.iter().map(LeaseEntryDTO::from).collect(),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_lease_lookup(
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(ns): Extension<Namespace>,
    Path(lease_id): Path<String>,
) -> Result<Response, Error> {
    let lease = expiration_manager
        .lookup(&lease_id, &ns.id)
        .await?
        .ok_or_else(|| ErrorType::NotFound(format!("Lease `{lease_id}` not found")))?;
    let resp = LookupLeaseResponse {
        lease: LeaseEntryDTO::from(&lease),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_lease_renew(
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(ns): Extension<Namespace>,
    Json(body): Json<RenewLeaseParams>,
    Path(lease_id): Path<String>,
) -> Result<Response, Error> {
    let lease = expiration_manager
        .renew_lease_entry(&lease_id, &ns.id, body.ttl)
        .await?;
    let resp = RenewLeaseResponse {
        lease: LeaseEntryDTO::from(&lease),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
