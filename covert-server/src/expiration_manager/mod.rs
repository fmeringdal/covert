pub mod clock;
mod lease;

use std::collections::HashMap;
use std::{cmp::Reverse, collections::BinaryHeap, sync::Arc};

use chrono::Utc;
use covert_types::auth::AuthPolicy;
use covert_types::error::ApiError;
use covert_types::methods::psql::RenewLeaseResponse;
use covert_types::request::{Operation, Request};
use covert_types::state::VaultState;
use futures::stream::FuturesOrdered;
use futures::StreamExt;
use hyper::http;
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

pub use self::lease::LeaseEntry;

use super::router::Router;

pub struct ExpirationManager {
    // Binary min-heap
    pending: Mutex<BinaryHeap<Reverse<LeaseEntry>>>,
    /// Notifies the background task handling entry expiration. The background
    /// task waits on this to be notified, then checks for expired values or the
    /// shutdown signal.
    background_task: Notify,
    router: Arc<Router>,
    lease_store: Arc<LeaseStore>,
    mount_store: Arc<MountStore>,
    shutdown_rx: Arc<RwLock<tokio::sync::mpsc::Receiver<()>>>,
    shutdown_tx: tokio::sync::mpsc::Sender<()>,
    revocation_retry_timeout: std::time::Duration,
    revocation_max_retries: usize,
}

impl ExpirationManager {
    pub fn new(
        router: Arc<Router>,
        lease_store: Arc<LeaseStore>,
        mount_store: Arc<MountStore>,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1);

        ExpirationManager {
            pending: Mutex::new(BinaryHeap::new()),
            background_task: Notify::new(),
            router,
            lease_store,
            mount_store,
            shutdown_rx: Arc::new(RwLock::new(rx)),
            shutdown_tx: tx,
            revocation_retry_timeout: std::time::Duration::from_millis(100),
            revocation_max_retries: 10,
        }
    }

    pub async fn register(&self, le: LeaseEntry) -> Result<(), Error> {
        self.lease_store.create(&le).await?;

        let mut pending = self.pending.lock().await;

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
                let delta = le.expires_at.timestamp_millis() - now.timestamp_millis();
                let when = Instant::now()
                    + std::time::Duration::from_millis(u64::try_from(delta).unwrap_or(u64::MAX));
                return Some(when);
            }

            // Check if lease might have been deleted
            match self.lease_store.lookup(&le.id).await {
                Ok(Some(le_from_store)) => {
                    if let Some(Reverse(le)) = pending.pop() {
                        if le_from_store.expires_at == le.expires_at
                            && le_from_store.issued_mount_path == le.issued_mount_path
                        {
                            let router = Arc::clone(&self.router);
                            let lease_store = Arc::clone(&self.lease_store);
                            let revocation_retry_timeout = self.revocation_retry_timeout;
                            let max_retries = self.revocation_max_retries;

                            tokio::spawn(async move {
                                let lease_id = le.id.clone();
                                let revoke_path = le.revoke_path.clone();

                                let mut retries = 0;

                                while let Err(error) =
                                    Self::revoke_lease_entry(&router, &lease_store, le.clone())
                                        .await
                                {
                                    retries += 1;
                                    tracing::error!(
                                        ?error,
                                        lease_id,
                                        revoke_path,
                                        retries,
                                        max_retries,
                                        "Failed to revoke lease."
                                    );

                                    // TODO: exp backoff
                                    tokio::time::sleep(revocation_retry_timeout).await;
                                    if retries >= max_retries {
                                        tracing::error!(
                                            ?error,
                                            lease_id,
                                            revoke_path,
                                            "Unable to revoke lease after max number of retries."
                                        );
                                        break;
                                    }
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

    #[tracing::instrument(skip_all, fields(lease_id = le.id, issued_mount_path = le.issued_mount_path))]
    async fn revoke_lease_entry(
        router: &Router,
        lease_store: &LeaseStore,
        le: LeaseEntry,
    ) -> Result<LeaseEntry, ApiError> {
        let lease_id = le.id.clone();

        // TODO: a better solution here is to just say that revoke endpoints
        // should be idempotent which should not be a problem. This delete
        // currently ensures that revoke endpoint are called at max once.
        match lease_store.delete(&lease_id).await {
            Ok(true) => (),
            // Might have already been deleted
            Ok(false) => return Ok(le),
            Err(error) => {
                tracing::error!(?error, "Failed to delete lease from the lease store");
                return Err(ApiError::internal_error());
            }
        }

        // Perform revocation
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Root);
        extensions.insert(VaultState::Unsealed);

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
            Ok(_) => Ok(le),
            Err(error) => {
                tracing::error!(?error, "Backend failed to revoke lease");
                if let Err(error) = lease_store.create(&le).await {
                    tracing::error!(?error, "Failed to add lease entry back to the lease store");
                }
                Err(error)
            }
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

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use covert_framework::{Backend, SyncService};
    use covert_types::{
        backend::{BackendCategory, BackendType},
        mount::{MountConfig, MountEntry},
        response::Response,
    };
    use tokio::time::sleep;

    use crate::{core::SYSTEM_MOUNT_PATH, router::RouteEntry, store::mount_store::tests::pool};

    use super::*;

    async fn secret_engine_handle(
        req: Request,
        recorder: Arc<RequestRecorder>,
        renew_ttl: Option<std::time::Duration>,
    ) -> Result<Response, ApiError> {
        let mut requests = recorder.0.write().await;
        requests.push(RequestInfo {
            path: req.path.clone(),
            operation: req.operation,
            reveived_at: Some(Utc::now()),
        });
        drop(requests);

        if req.path == "creds" {
            match req.operation {
                Operation::Revoke => Ok(Response::ok()),
                Operation::Renew => {
                    let data = RenewLeaseResponse {
                        ttl: renew_ttl.unwrap(),
                    };
                    Ok(Response::Raw(serde_json::to_value(data).unwrap()))
                }
                _ => Err(ApiError::not_found()),
            }
        } else if req.path == "creds-slow" {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            match req.operation {
                Operation::Revoke => Ok(Response::ok()),
                Operation::Renew => {
                    let data = RenewLeaseResponse {
                        ttl: renew_ttl.unwrap(),
                    };
                    Ok(Response::Raw(serde_json::to_value(data).unwrap()))
                }
                _ => Err(ApiError::not_found()),
            }
        } else {
            Err(ApiError::not_found())
        }
    }

    async fn system_handle(
        req: Request,
        recorder: Arc<RequestRecorder>,
        renew_ttl: Option<std::time::Duration>,
    ) -> Result<Response, ApiError> {
        let mut requests = recorder.0.write().await;
        requests.push(RequestInfo {
            path: req.path.clone(),
            operation: req.operation,
            reveived_at: Some(Utc::now()),
        });
        drop(requests);

        if req.path == "token/revoke" {
            match req.operation {
                Operation::Revoke => Ok(Response::ok()),
                _ => Err(ApiError::not_found()),
            }
        } else if req.path == "token/renew" {
            let data = RenewLeaseResponse {
                ttl: renew_ttl.unwrap(),
            };
            match req.operation {
                Operation::Renew => Ok(Response::Raw(serde_json::to_value(data).unwrap())),
                _ => Err(ApiError::not_found()),
            }
        } else {
            Err(ApiError::not_found())
        }
    }

    #[derive(Debug, Clone)]
    pub struct RequestInfo {
        pub path: String,
        pub operation: Operation,
        pub reveived_at: Option<DateTime<Utc>>,
    }

    impl PartialEq for RequestInfo {
        fn eq(&self, other: &Self) -> bool {
            if self.path != other.path {
                return false;
            }

            if self.operation != other.operation {
                return false;
            }

            let received_at = if let Some(dt) = self.reveived_at {
                dt
            } else {
                return true;
            };
            let other_received_at = if let Some(dt) = other.reveived_at {
                dt
            } else {
                return true;
            };

            let threshold_millis = 50;
            let diff =
                (received_at.timestamp_millis() - other_received_at.timestamp_millis()).abs();

            diff <= threshold_millis
        }
    }

    pub struct RequestRecorder(RwLock<Vec<RequestInfo>>);

    #[tokio::test]
    async fn revoke_secret_after_ttl_expires() {
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);

        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        sleep(std::time::Duration::from_millis(100)).await;

        // Setup mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            path: "foo/".to_string(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            async move { secret_engine_handle(req, recorder, None).await }
        }));
        let backend = Arc::new(Backend {
            category: BackendCategory::Logical,
            migrations: vec![],
            variant: me.backend_type,
            handler,
        });
        let re = RouteEntry::new(
            Uuid::new_v4(),
            me.path.clone(),
            backend,
            MountConfig::default(),
        )
        .unwrap();
        router.mount(re).await.unwrap();

        let ttl_millis = 10;
        let le = LeaseEntry::new(
            me.path.clone(),
            Some("creds".into()),
            &(),
            Some("creds".into()),
            &(),
            chrono::Duration::milliseconds(ttl_millis),
        )
        .unwrap();

        assert!(exp_m.register(le.clone()).await.is_ok());
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);

        tokio::time::sleep(std::time::Duration::from_millis(
            u64::try_from(ttl_millis * 4).unwrap(),
        ))
        .await;

        let requests = recorder.0.read().await;
        assert_eq!(
            *requests,
            vec![RequestInfo {
                path: "creds".into(),
                operation: Operation::Revoke,
                reveived_at: Some(le.expires_at)
            }]
        );

        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);
    }

    #[tokio::test]
    async fn revoke_token_after_ttl_expires() {
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::System,
            config: MountConfig::default(),
            path: SYSTEM_MOUNT_PATH.to_string(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            async move { system_handle(req, recorder, None).await }
        }));
        let backend = Arc::new(Backend {
            category: BackendCategory::Logical,
            migrations: vec![],
            variant: me.backend_type,
            handler,
        });
        let re = RouteEntry::new(
            Uuid::new_v4(),
            me.path.clone(),
            backend,
            MountConfig::default(),
        )
        .unwrap();
        router.mount(re).await.unwrap();

        let ttl_millis = 10;
        let le = LeaseEntry::new(
            me.path.clone(),
            None,
            &(),
            None,
            &(),
            chrono::Duration::milliseconds(ttl_millis),
        )
        .unwrap();

        assert!(exp_m.register(le.clone()).await.is_ok());
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);
        tokio::time::sleep(std::time::Duration::from_millis(
            u64::try_from(ttl_millis * 4).unwrap(),
        ))
        .await;

        let requests = recorder.0.read().await;
        assert_eq!(
            *requests,
            vec![RequestInfo {
                path: "token/revoke".into(),
                operation: Operation::Revoke,
                reveived_at: Some(le.expires_at)
            }]
        );

        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);
    }

    #[tokio::test]
    async fn revoke_before_ttl_expires() {
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::System,
            config: MountConfig::default(),
            path: SYSTEM_MOUNT_PATH.to_string(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            async move { system_handle(req, recorder, None).await }
        }));
        let backend = Arc::new(Backend {
            category: BackendCategory::Logical,
            migrations: vec![],
            variant: me.backend_type,
            handler,
        });
        let re = RouteEntry::new(
            Uuid::new_v4(),
            me.path.clone(),
            backend,
            MountConfig::default(),
        )
        .unwrap();
        router.mount(re).await.unwrap();

        let ttl_millis = 10;
        let le = LeaseEntry::new(
            me.path.clone(),
            None,
            &(),
            None,
            &(),
            chrono::Duration::milliseconds(ttl_millis),
        )
        .unwrap();

        assert!(exp_m.register(le.clone()).await.is_ok());
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);
        assert!(exp_m.revoke_lease_entry_by_id(le.id()).await.is_ok());
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);

        tokio::time::sleep(std::time::Duration::from_millis(
            u64::try_from(ttl_millis * 2).unwrap(),
        ))
        .await;

        let requests = recorder.0.read().await;
        assert_eq!(
            *requests,
            vec![RequestInfo {
                path: "token/revoke".into(),
                operation: Operation::Revoke,
                reveived_at: None
            }]
        );

        // Should be no pending leases
        let pending = exp_m.pending.lock().await;
        assert!(pending.peek().is_none());
        drop(pending);

        // Sanity test that leases is still empty
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);
    }

    #[tokio::test]
    async fn renew() {
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            path: "psql/".into(),
        };
        mount_store.create(&me).await.unwrap();

        let renew_ttl = std::time::Duration::from_millis(200);

        let recorder_moved = Arc::clone(&recorder);
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            async move { secret_engine_handle(req, recorder, Some(renew_ttl)).await }
        }));
        let backend = Arc::new(Backend {
            category: BackendCategory::Logical,
            migrations: vec![],
            variant: me.backend_type,
            handler,
        });
        let re = RouteEntry::new(
            Uuid::new_v4(),
            me.path.clone(),
            backend,
            MountConfig::default(),
        )
        .unwrap();
        router.mount(re).await.unwrap();

        let ttl_millis = 100;
        let le = LeaseEntry::new(
            me.path.clone(),
            Some("creds".into()),
            &(),
            Some("creds".into()),
            &(),
            chrono::Duration::milliseconds(ttl_millis),
        )
        .unwrap();

        assert!(exp_m.register(le.clone()).await.is_ok());
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);

        // Renew
        let new_le = exp_m.renew_lease_entry(le.id()).await.unwrap();
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);
        let new_expire_time = new_le.expires_at;

        tokio::time::sleep(std::time::Duration::from_millis(
            u64::try_from(ttl_millis * 2).unwrap(),
        ))
        .await;

        let requests = recorder.0.read().await;
        assert_eq!(
            *requests,
            vec![RequestInfo {
                path: "creds".into(),
                operation: Operation::Renew,
                reveived_at: None
            }]
        );
        drop(requests);

        tokio::time::sleep(renew_ttl).await;

        // Now it should be revoked
        let requests = recorder.0.read().await;
        assert_eq!(
            *requests,
            vec![
                RequestInfo {
                    path: "creds".into(),
                    operation: Operation::Renew,
                    reveived_at: None
                },
                RequestInfo {
                    path: "creds".into(),
                    operation: Operation::Revoke,
                    reveived_at: Some(new_expire_time)
                }
            ]
        );
        drop(requests);

        // Should be no pending leases
        let pending = exp_m.pending.lock().await;
        assert!(pending.peek().is_none());
        drop(pending);

        // Sanity test that leases is still empty
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);
    }

    #[tokio::test]
    async fn retry_failed_revocation() {
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let mut exp_m = ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
        );
        exp_m.revocation_retry_timeout = std::time::Duration::from_millis(10);
        exp_m.revocation_max_retries = 5;
        let exp_m = Arc::new(exp_m);

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            path: "psql/".into(),
        };
        mount_store.create(&me).await.unwrap();

        let renew_ttl = std::time::Duration::from_millis(100);

        let recorder_moved = Arc::clone(&recorder);
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            async move { secret_engine_handle(req, recorder, Some(renew_ttl)).await }
        }));
        let backend = Arc::new(Backend {
            category: BackendCategory::Logical,
            migrations: vec![],
            variant: me.backend_type,
            handler,
        });
        let re = RouteEntry::new(
            Uuid::new_v4(),
            me.path.clone(),
            backend,
            MountConfig::default(),
        )
        .unwrap();
        router.mount(re).await.unwrap();

        let ttl_millis = 10;
        let le = LeaseEntry::new(
            me.path.clone(),
            // This will ensure revocation returns 404 error
            Some("invalid-revoke-path".into()),
            &(),
            Some("creds".into()),
            &(),
            chrono::Duration::milliseconds(ttl_millis),
        )
        .unwrap();

        assert!(exp_m.register(le.clone()).await.is_ok());

        for _ in 0..exp_m.revocation_max_retries * 2 {
            tokio::time::sleep(exp_m.revocation_retry_timeout).await;
        }

        let requests = recorder.0.read().await;
        assert_eq!(requests.len(), exp_m.revocation_max_retries);
        drop(requests);

        // Lease should stil be stored
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);
    }

    #[tokio::test]
    async fn revoke_for_mount() {
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            path: "psql/".into(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            async move { secret_engine_handle(req, recorder, None).await }
        }));
        let backend = Arc::new(Backend {
            category: BackendCategory::Logical,
            migrations: vec![],
            variant: me.backend_type,
            handler,
        });
        let re = RouteEntry::new(
            Uuid::new_v4(),
            me.path.clone(),
            backend,
            MountConfig::default(),
        )
        .unwrap();
        router.mount(re).await.unwrap();

        let ttl_millis = 100;
        let lease_count = 50;

        for _ in 0..lease_count {
            let le = LeaseEntry::new(
                me.path.clone(),
                Some("creds".into()),
                &(),
                Some("creds".into()),
                &(),
                chrono::Duration::milliseconds(ttl_millis),
            )
            .unwrap();
            assert!(exp_m.register(le.clone()).await.is_ok());
        }

        assert_eq!(
            exp_m
                .revoke_leases_by_mount_prefix(&me.path)
                .await
                .unwrap()
                .len(),
            lease_count
        );

        // Leases should be empty
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);

        // The leases can still be in the pending queue until their TTL has expired
        tokio::time::sleep(std::time::Duration::from_millis(
            u64::try_from(ttl_millis * 2).unwrap(),
        ))
        .await;
        let pending = exp_m.pending.lock().await;
        assert!(pending.peek().is_none());
        drop(pending);

        let requests = recorder.0.read().await;
        assert_eq!(requests.len(), lease_count);
        drop(requests);
    }

    #[tokio::test]
    async fn slow_revoke_endpoint_does_not_halt_other_revocations() {
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            path: "psql/".into(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            async move { secret_engine_handle(req, recorder, None).await }
        }));
        let backend = Arc::new(Backend {
            category: BackendCategory::Logical,
            migrations: vec![],
            variant: me.backend_type,
            handler,
        });
        let re = RouteEntry::new(
            Uuid::new_v4(),
            me.path.clone(),
            backend,
            MountConfig::default(),
        )
        .unwrap();
        router.mount(re).await.unwrap();

        let ttl_millis = 50;
        let lease_count = 5;

        for i in 0..lease_count {
            let le = LeaseEntry::new(
                me.path.clone(),
                Some("creds".into()),
                &i,
                Some("creds".into()),
                &i,
                chrono::Duration::milliseconds(ttl_millis),
            )
            .unwrap();
            assert!(exp_m.register(le.clone()).await.is_ok());
        }

        // Register lease that will be slow to revoke
        let le = LeaseEntry::new(
            me.path.clone(),
            Some("creds-slow".into()),
            &(),
            Some("creds".into()),
            &(),
            chrono::Duration::milliseconds(2),
        )
        .unwrap();
        assert!(exp_m.register(le.clone()).await.is_ok());

        tokio::time::sleep(std::time::Duration::from_millis(
            u64::try_from(ttl_millis * 2).unwrap(),
        ))
        .await;

        // Leases should be empty
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);

        let pending = exp_m.pending.lock().await;
        assert!(pending.peek().is_none());
        drop(pending);

        let requests = recorder.0.read().await;
        assert_eq!(requests.len(), lease_count + 1);
        drop(requests);
    }
}
