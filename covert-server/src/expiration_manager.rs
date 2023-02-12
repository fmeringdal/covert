use std::collections::HashMap;
use std::{cmp::Reverse, collections::BinaryHeap, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use covert_types::auth::AuthPolicy;
use covert_types::error::ApiError;
use covert_types::methods::psql::RenewLeaseResponse;
use covert_types::request::{Operation, Request};
use covert_types::state::VaultState;
use futures::stream::FuturesOrdered;
use futures::StreamExt;
use hyper::http;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tokio::{
    sync::Notify,
    time::{self, Instant},
};
use tracing::info;
use uuid::Uuid;

use crate::error::{Error, ErrorType};
use crate::store::lease_store::LeaseStore;
use crate::store::mount_store::MountStore;

use super::router::Router;

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
}

impl LeaseEntry {
    pub fn new<T: Serialize>(
        issued_mount_path: String,
        revoke_path: Option<String>,
        revoke_data: &T,
        renew_path: Option<String>,
        renew_data: &T,
        ttl: Duration,
    ) -> Result<Self, Error> {
        let now = Utc::now();
        let expire_time = now + ttl;
        let issue_time = now;
        let last_renewal_time = now;

        let lease_id = format!("{}", Uuid::new_v4());
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
            issued_at: issue_time,
            expires_at: expire_time,
            last_renewal_time,
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

pub struct ExpirationManager {
    // Binary min-heap
    pending: Mutex<BinaryHeap<Reverse<LeaseEntry>>>,
    /// Notifies the background task handling entry expiration. The background
    /// task waits on this to be notified, then checks for expired values or the
    /// shutdown signal.
    background_task: Notify,
    router: Arc<Router>,
    /// Lease store
    lease_store: Arc<LeaseStore>,
    mount_store: Arc<MountStore>,
    shutdown_rx: Arc<RwLock<tokio::sync::mpsc::Receiver<()>>>,
    shutdown_tx: tokio::sync::mpsc::Sender<()>,
}

impl ExpirationManager {
    pub fn new(
        router: Arc<Router>,
        lease_store: Arc<LeaseStore>,
        mount_store: Arc<MountStore>,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1);

        Self {
            pending: Mutex::new(BinaryHeap::new()),
            background_task: Notify::new(),
            router,
            lease_store,
            mount_store,
            shutdown_rx: Arc::new(RwLock::new(rx)),
            shutdown_tx: tx,
        }
    }

    pub async fn register(&self, le: LeaseEntry) -> Result<(), Error> {
        let mut pending = self.pending.lock().await;

        self.lease_store.create(&le).await?;

        // Only notify the worker task if the newly inserted expiration is the
        // **next** lease to revoke. In this case, the worker needs to be woken up
        // to update its state.
        let notify = pending.peek().map_or(true, |next| next.0 > le);

        pending.push(Reverse(le));
        drop(pending);

        if notify {
            self.background_task.notify_one();
        }
        Ok(())
    }

    pub async fn revoke_leases_by_mount_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<LeaseEntry>, Error> {
        let leases = self.lease_store.list_by_mount(prefix).await?;

        let mut revoke_futures = FuturesOrdered::new();

        for lease in leases {
            let fut = Self::revoke_lease_entry(&self.router, &self.lease_store, lease.clone());
            revoke_futures.push_back(fut);
        }

        let revoked_leases = revoke_futures
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|res| match res {
                Ok(le) => Some(le),
                Err(error) => {
                    tracing::error!(?error, "Failed to revoke lease");
                    None
                }
            })
            .collect::<Vec<_>>();

        Ok(revoked_leases)
    }

    pub async fn list_by_mount_prefix(&self, prefix: &str) -> Result<Vec<LeaseEntry>, Error> {
        self.lease_store.list_by_mount(prefix).await
    }

    pub async fn lookup(&self, lease_id: &str) -> Result<Option<LeaseEntry>, Error> {
        self.lease_store.lookup(lease_id).await
    }

    /// Revoke all leases and return the `Instant` at which the **next**
    /// lease expires. The background task will sleep until this instant.
    async fn revoke_leases(&self) -> Option<Instant> {
        // Find all keys scheduled to expire **before** now.
        let now = Utc::now();
        let mut pending = self.pending.lock().await;

        while let Some(le) = pending.peek().map(|j| &j.0) {
            if le.expires_at > now {
                // Done revoking, `when` is the instant at which the next key
                // expires. The worker task will wait until this instant.
                // let delta = now - le.expire_time;
                // let when = Instant::now() + delta.to_std().unwrap();
                let delta = le.expires_at.timestamp_millis() - now.timestamp_millis();
                let when = Instant::now()
                    + std::time::Duration::from_millis(u64::try_from(delta).unwrap_or(u64::MAX));
                return Some(when);
            }

            // Check if lease might have been deleted
            match self.lease_store.lookup(&le.id).await {
                Ok(Some(le_from_store)) => {
                    if let Some(Reverse(le)) = pending.pop() {
                        if le_from_store == le {
                            let router = Arc::clone(&self.router);
                            let lease_store = Arc::clone(&self.lease_store);
                            tokio::spawn(async move {
                                let lease_id = le.id.clone();
                                let revoke_path = le.revoke_path.clone();
                                if let Err(error) =
                                    Self::revoke_lease_entry(&router, &lease_store, le).await
                                {
                                    tracing::error!(
                                        ?error,
                                        lease_id,
                                        revoke_path,
                                        "Unable to revoke lease."
                                    );
                                }
                            });
                        } else {
                            // Might have been renewed and some fields could have changed so
                            // add it back without revoking.
                            pending.push(Reverse(le_from_store));
                        }
                    }
                }
                Ok(None) => {
                    // Probably already revoked
                    pending.pop();
                }
                Err(error) => {
                    tracing::error!(?error, ?le, "Unable to lookup lease");
                }
            };
        }

        None
    }

    pub async fn revoke_lease_entry_by_id(&self, lease_id: &str) -> Result<LeaseEntry, Error> {
        let le = self
            .lookup(lease_id)
            .await?
            .ok_or_else(|| ErrorType::NotFound(format!("Lease `{lease_id}` not found")))?;

        Self::revoke_lease_entry(&self.router, &self.lease_store, le)
            .await
            .map_err(|error| {
                tracing::error!(?error, lease_id, "Unable to revoke lease.");
                ErrorType::RevokeLease {
                    source: Box::new(error),
                    lease_id: lease_id.to_string(),
                }
                .into()
            })
    }

    async fn revoke_lease_entry(
        router: &Router,
        lease_store: &LeaseStore,
        le: LeaseEntry,
    ) -> Result<LeaseEntry, ApiError> {
        // Perform revocation
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Root);
        extensions.insert(VaultState::Unsealed);
        let lease_id = le.id.clone();

        let revoke_path = le.revoke_path.as_ref().map_or_else(
            || "sys/token/revoke".into(),
            |revoke_path| format!("{}{revoke_path}", le.issued_mount_path),
        );

        let req = Request {
            id: Uuid::default(),
            operation: Operation::Revoke,
            path: revoke_path,
            data: le.revoke_data.clone().into(),
            extensions,
            token: None,
            is_sudo: true,
            params: Vec::default(),
            query_string: String::default(),
            headers: HashMap::default(),
        };

        match router.route(req).await {
            Ok(_) => {
                if lease_store.delete(&lease_id).await.is_err() {
                    tracing::error!(lease_id, "Failed to delete lease from the lease store");
                }
                Ok(le)
            }
            Err(error) => Err(error),
        }
    }

    pub async fn renew_lease_entry(&self, lease_id: &str) -> Result<LeaseEntry, Error> {
        let mut le = self
            .lease_store
            .lookup(lease_id)
            .await?
            .ok_or_else(|| ErrorType::NotFound(format!("Lease `{lease_id}` not found")))?;
        let mount_config = self
            .mount_store
            .get_by_path(&le.issued_mount_path)
            .await?
            .ok_or_else(|| ErrorType::MountNotFound {
                path: le.issued_mount_path.clone(),
            })?
            .config;

        // Perform renewal
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Root);
        extensions.insert(VaultState::Unsealed);

        let renew_path = le
            .renew_path
            .as_ref()
            // TODO: token renew endpoint does not exist yet
            .map_or_else(
                || "sys/token/renew".into(),
                |renew_path| format!("{}{renew_path}", le.issued_mount_path),
            );
        let req = Request {
            id: Uuid::default(),
            operation: Operation::Renew,
            path: renew_path,
            data: le.renew_data.clone().into(),
            extensions,
            token: None,
            is_sudo: true,
            params: Vec::default(),
            query_string: String::default(),
            headers: HashMap::default(),
        };

        let router = Arc::clone(&self.router);

        let renew_path = req.path.clone();
        match router.route(req).await {
            Ok(resp) => {
                let resp = resp.response.data::<RenewLeaseResponse>().map_err(|_| {
                    ErrorType::InternalError(anyhow::Error::msg("Unexpected renew response"))
                })?;
                let ttl = if resp.ttl > mount_config.max_lease_ttl {
                    mount_config.max_lease_ttl
                } else {
                    resp.ttl
                };

                le.expires_at = Utc::now()
                    + chrono::Duration::from_std(ttl).map_err(|_| {
                        ErrorType::InternalError(anyhow::Error::msg(
                            "Unable to create TTL from renew response",
                        ))
                    })?;
                le.last_renewal_time = Utc::now();
                self.lease_store
                    .renew(lease_id, le.expires_at, le.last_renewal_time)
                    .await?;

                Ok(le)
            }
            Err(error) => {
                tracing::error!(?error, lease_id, renew_path, "Unable to renew lease.");
                Err(ErrorType::RenewLease {
                    source: Box::new(error),
                    lease_id: lease_id.to_string(),
                }
                .into())
            }
        }
    }

    pub async fn start(&self) -> Result<(), Error> {
        // Initialize leases from storage
        let leases = self.lease_store.list().await?;
        for lease in leases {
            self.register(lease).await?;
        }

        loop {
            let mut shutdown_rx = self.shutdown_rx.write().await;
            if let Some(when) = self.revoke_leases().await {
                tokio::select! {
                        _ = time::sleep_until(when) => {}
                        _ = self.background_task.notified() => {}
                        _ = shutdown_rx.recv() => {
                            info!("Expiration manager received shutdown signal");
                            break;
                        }
                }
            } else {
                // There are no leases expiring in the future. Wait until the task is
                // notified or shutdown signal is received.
                tokio::select! {
                        _ = self.background_task.notified() => {}
                        _ = shutdown_rx.recv() => {
                            info!("Expiration manager received shutdown signal");
                            break;
                        }
                }
            }
        }
        info!("Expiration manager shutting down");
        Ok(())
    }

    pub async fn stop(&self) {
        // TODO: wait for expiration manager to shutdown fully.
        let _ = self.shutdown_tx.send(()).await;
    }
}
