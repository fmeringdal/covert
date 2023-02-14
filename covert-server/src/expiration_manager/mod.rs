pub mod clock;
mod lease;

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use chrono::Duration;
use covert_types::auth::AuthPolicy;
use covert_types::error::ApiError;
use covert_types::methods::psql::RenewLeaseResponse;
use covert_types::methods::RenewLeaseParams;
use covert_types::request::{Operation, Request};
use covert_types::state::VaultState;
use covert_types::ttl::calculate_ttl;
use futures::stream::FuturesOrdered;
use futures::{Future, StreamExt};
use hyper::http;
use tokio::sync::Notify;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::error::{Error, ErrorType};
use crate::store::lease_store::LeaseStore;
use crate::store::mount_store::MountStore;

use self::clock::Clock;
pub use self::lease::LeaseEntry;

use super::router::Router;

/// The expiration manager is resposible for revoking and renewing leases.
pub struct ExpirationManager {
    /// Used to notify the revocation worker of when new leases are registered
    background_task: Notify,
    /// Router to send revoke / renew requests to the backends
    router: Arc<Router>,
    /// Lease storage
    lease_store: Arc<LeaseStore>,
    /// Mount storage
    mount_store: Arc<MountStore>,
    /// Shutdown listener
    shutdown_rx: Arc<RwLock<tokio::sync::mpsc::Receiver<()>>>,
    /// Shutdown transmitter
    shutdown_tx: tokio::sync::mpsc::Sender<()>,
    /// Time before retrying a failed revocation
    revocation_retry_timeout: Duration,
    /// Max number of revoke requests before the lease is deleted
    revocation_max_retries: u32,
    /// Timeout for the revoke endpoint
    revocation_timeout: std::time::Duration,
    /// Number of leases the revocation worker should try to revoke at the same time
    revocation_worker_concurrency: usize,
    /// Provides time information. Gives us deterministic time in tests.
    clock: Arc<dyn Clock>,
}

impl ExpirationManager {
    /// Create a new expiration manager.
    pub fn new(
        router: Arc<Router>,
        lease_store: Arc<LeaseStore>,
        mount_store: Arc<MountStore>,
        clock: impl Clock,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1);

        ExpirationManager {
            background_task: Notify::new(),
            router,
            lease_store,
            mount_store,
            shutdown_rx: Arc::new(RwLock::new(rx)),
            shutdown_tx: tx,
            revocation_retry_timeout: Duration::seconds(5),
            revocation_max_retries: 10,
            revocation_timeout: std::time::Duration::from_secs(10),
            revocation_worker_concurrency: 100,
            clock: Arc::new(clock),
        }
    }

    /// Register a new [`LeaseEntry`].
    ///
    /// This is the only way to register new leases, leases should *not* be inserted
    /// directly to the [`LeaseStore`] without going throught the expiration manager.
    pub async fn register(&self, le: LeaseEntry) -> Result<(), Error> {
        self.lease_store.create(&le).await?;
        // Let the revocation worker know about the lease.
        self.background_task.notify_one();
        Ok(())
    }

    /// Revoke all leases issued by mounts under a given path prefix.
    pub async fn revoke_leases_by_mount_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<LeaseEntry>, Error> {
        let leases = self.lease_store.list_by_mount_prefix(prefix).await?;

        let mut revoke_futures = FuturesOrdered::new();

        for lease in leases {
            revoke_futures
                .push_back(async move { self.revoke_lease_entry(&lease).await.map(|_| lease) });
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

    /// List all leases issued by mounts under a given path prefix.
    pub async fn list_by_mount_prefix(&self, prefix: &str) -> Result<Vec<LeaseEntry>, Error> {
        self.lease_store.list_by_mount_prefix(prefix).await
    }

    /// Lookup a lease by its id.
    pub async fn lookup(&self, lease_id: &str) -> Result<Option<LeaseEntry>, Error> {
        self.lease_store.lookup(lease_id).await
    }

    /// Revoke a lease by its id.
    pub async fn revoke_lease_entry_by_id(&self, lease_id: &str) -> Result<LeaseEntry, Error> {
        let le = self
            .lookup(lease_id)
            .await?
            .ok_or_else(|| ErrorType::NotFound(format!("Lease `{lease_id}` not found")))?;

        self.revoke_lease_entry(&le)
            .await
            .map(|_| le)
            .map_err(|error| {
                tracing::error!(?error, lease_id, "Unable to revoke lease.");
                ErrorType::RevokeLease {
                    source: Box::new(error),
                    lease_id: lease_id.to_string(),
                }
                .into()
            })
    }

    /// Send a revoke request to the backend that is resposible for revoking the
    /// leased data.
    #[tracing::instrument(skip_all, fields(lease_id = le.id, issued_mount_path = le.issued_mount_path))]
    async fn send_lease_revoke_request(&self, le: &LeaseEntry) -> Result<(), ApiError> {
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

        match timeout(self.revocation_timeout, self.router.route(req)).await {
            Ok(backend_resp) => backend_resp.map(|_| ()).map_err(|error| {
                tracing::error!(?error, "Backend failed to revoke lease");
                error
            }),
            Err(_) => Err(ApiError::timeout()),
        }
    }

    /// Renew a lease by its id.
    pub async fn renew_lease_entry(
        &self,
        lease_id: &str,
        ttl: Option<std::time::Duration>,
    ) -> Result<LeaseEntry, Error> {
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

        let ttl =
            calculate_ttl(self.clock.now(), le.issued_at, &mount_config, ttl).map_err(|_| {
                ErrorType::InternalError(anyhow::Error::msg(
                    "Failed to calculate TTL when renewing lease",
                ))
            })?;

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

        let data = RenewLeaseParams {
            ttl: ttl
                .to_std()
                .map_err(|_| ErrorType::BadRequest("Bad renew TTL".into()))?,
            data: le.renew_data.clone(),
        };
        let data = serde_json::to_vec(&data)
            .map_err(|_| ErrorType::BadRequest("Bad renew payload".into()))?;

        let req = Request {
            id: Uuid::default(),
            operation: Operation::Renew,
            path: renew_path,
            data: data.into(),
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

                let ttl = calculate_ttl(
                    self.clock.now(),
                    le.issued_at,
                    &mount_config,
                    Some(resp.ttl),
                )
                .map_err(|_| {
                    ErrorType::InternalError(anyhow::Error::msg(
                        "Failed to calculate TTL when renewing lease",
                    ))
                })?;

                let now = self.clock.now();

                le.expires_at = now + ttl;
                le.last_renewal_time = now;
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

    /// Start the revocation worker.
    #[tracing::instrument(skip(self), name = "start_expiration_manager")]
    pub async fn start(&self) -> Result<(), Error> {
        let mut shutdown_rx = self.shutdown_rx.write().await;

        loop {
            let now = self.clock.now();
            #[allow(clippy::cast_possible_truncation)]
            let leases = match self
                .lease_store
                .pull(self.revocation_worker_concurrency as u32, now)
                .await
            {
                Ok(leases) => leases,
                Err(error) => {
                    error!(?error, "Failed to pull leases for revocation");
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    continue;
                }
            };

            let number_of_leases = leases.len();
            if number_of_leases == 0 {
                // TODO: this might need more care to ensure no leases are lost
                let next_lease_fut = self
                    .lease_store
                    .peek()
                    .await?
                    .map(|le| le.expires_at - self.clock.now())
                    .and_then(|duration| duration.to_std().ok())
                    .map_or_else::<Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>, _, _>(
                        || Box::pin(std::future::pending()),
                        |duration| self.clock.sleep(duration),
                    );

                tokio::select! {
                        // If new lease is registered
                        _ = self.background_task.notified() => {
                            continue;
                        }
                        // Future that resolves when the next lease is ready
                        // to be revoked
                        _ = next_lease_fut => {
                            continue;
                        }
                        // Break loop on shutdown signal
                        _ = shutdown_rx.recv() => {
                            break;
                        }
                }
            }
            debug!("Fetched {} leases ready for revocation", number_of_leases);

            futures::stream::iter(leases)
                .for_each_concurrent(self.revocation_worker_concurrency, |le| async move {
                    // Errors are handled by this function, no more logging
                    // or error handling is required at this point.
                    let _ = self.revoke_lease_entry(&le).await;
                })
                .await;
        }

        info!("Expiration manager shutting down");
        Ok(())
    }

    /// Perform revocation of the [`LeaseEntry`].
    #[tracing::instrument(skip_all, fields(lease_id = le.id, mount_path = le.issued_mount_path))]
    async fn revoke_lease_entry(&self, le: &LeaseEntry) -> Result<(), Error> {
        let res = self.send_lease_revoke_request(le).await;
        match res {
            Ok(_) => {
                self.lease_store
                    .delete(&le.id)
                    .await
                    .map_err(|error| {
                        // **NOTE**: This means that revoke endpoints should be idempotent as
                        // this will trigger a new revoke request to be sent even though
                        // the lease was just revoked from the backend
                        tracing::error!(?error, "Failed to delete lease from the lease store");
                        error
                    })
                    .map(|_| ())
            }
            Err(error) => {
                error!(?error, "failed to revoke lease entry from backend");
                // TODO: why is +1 needed here
                if le.failed_revocation_attempts + 1 >= self.revocation_max_retries {
                    // Delete from store
                    if let Err(error) = self.lease_store.delete(&le.id).await {
                        error!(?error, "failed to delete lease from store that has passed max number of revocation retries");
                    };
                } else {
                    // Increase failed count
                    if let Err(error) = self
                        .lease_store
                        .increment_failed_revocation_attempts(
                            &le.id,
                            // TODO: exp backoff and configure revocation_retry_timeout
                            le.expires_at + self.revocation_retry_timeout,
                        )
                        .await
                    {
                        error!(?error, "failed to delete lease from store that has passed max number of revocation retries");
                    }
                }
                Err(ErrorType::InternalError(error.into()).into())
            }
        }
    }

    /// Shutdown the expiration manager.
    #[tracing::instrument(skip(self), name = "stop_expiration_manager")]
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

    use crate::{
        core::SYSTEM_MOUNT_PATH, expiration_manager::clock::test::TestClock, router::RouteEntry,
        store::mount_store::tests::pool,
    };

    use super::*;

    async fn secret_engine_handle(
        req: Request,
        recorder: Arc<RequestRecorder>,
        renew_ttl: Option<std::time::Duration>,
        clock: TestClock,
    ) -> Result<Response, ApiError> {
        let mut requests = recorder.0.write().await;
        requests.push(RequestInfo {
            path: req.path.clone(),
            operation: req.operation,
            reveived_at: Some(clock.now()),
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
        clock: TestClock,
    ) -> Result<Response, ApiError> {
        let mut requests = recorder.0.write().await;
        requests.push(RequestInfo {
            path: req.path.clone(),
            operation: req.operation,
            reveived_at: Some(clock.now()),
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

            received_at == other_received_at
        }
    }

    pub struct RequestRecorder(RwLock<Vec<RequestInfo>>);

    async fn advance(clock: &TestClock, duration: Duration) {
        clock.advance(duration.num_milliseconds());
        // Yield and give some time for expiration manager to wake up and revoke
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    async fn advance_to(clock: &TestClock, duration: DateTime<Utc>) {
        clock.set(duration.timestamp_millis());
        // Yield and give some time for expiration manager to wake up and revoke
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn revoke_secret_after_ttl_expires() {
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));
        let clock = TestClock::new();

        let pool = Arc::new(pool().await);

        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
            clock.clone(),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        sleep(std::time::Duration::ZERO).await;

        // Setup mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            path: "foo/".to_string(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let clock_moved = clock.clone();
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            let clock = clock_moved.clone();
            async move { secret_engine_handle(req, recorder, None, clock).await }
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

        let ttl = Duration::hours(4);
        let le = LeaseEntry::new(
            me.path.clone(),
            Some("creds".into()),
            &(),
            Some("creds".into()),
            &(),
            clock.now(),
            ttl,
        )
        .unwrap();

        assert!(exp_m.register(le.clone()).await.is_ok());
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);
        let next_lease = lease_store.peek().await.unwrap();
        assert_eq!(next_lease, Some(le.clone()));

        // Wait ttl - 1 hours and it should still be there
        advance_to(&clock, le.expires_at - Duration::hours(1)).await;
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);

        // Go to expire time
        advance_to(&clock, le.expires_at).await;

        let requests = recorder.0.read().await;
        assert_eq!(
            *requests,
            vec![RequestInfo {
                path: "creds".into(),
                operation: Operation::Revoke,
                reveived_at: Some(clock.now())
            }]
        );

        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);
    }

    #[tokio::test]
    async fn revoke_token_after_ttl_expires() {
        let clock = TestClock::new();
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
            clock.clone(),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::ZERO).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::System,
            config: MountConfig::default(),
            path: SYSTEM_MOUNT_PATH.to_string(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let clock_moved = clock.clone();
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            let clock = clock_moved.clone();
            async move { system_handle(req, recorder, None, clock).await }
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

        let ttl = Duration::hours(4);
        let le = LeaseEntry::new(me.path.clone(), None, &(), None, &(), clock.now(), ttl).unwrap();

        assert!(exp_m.register(le.clone()).await.is_ok());
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);

        // Wait ttl - 1 hours and it should still be there
        advance_to(&clock, le.expires_at - Duration::hours(1)).await;
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);

        // Go to revocation time
        advance_to(&clock, le.expires_at).await;

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
        let clock = TestClock::new();
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
            clock.clone(),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::ZERO).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::System,
            config: MountConfig::default(),
            path: SYSTEM_MOUNT_PATH.to_string(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let clock_moved = clock.clone();
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            let clock = clock_moved.clone();
            async move { system_handle(req, recorder, None, clock).await }
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

        let ttl = Duration::hours(4);
        let le = LeaseEntry::new(me.path.clone(), None, &(), None, &(), clock.now(), ttl).unwrap();

        assert!(exp_m.register(le.clone()).await.is_ok());
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);
        assert!(exp_m.revoke_lease_entry_by_id(le.id()).await.is_ok());
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);

        advance_to(&clock, le.expires_at).await;

        let requests = recorder.0.read().await;
        assert_eq!(
            *requests,
            vec![RequestInfo {
                path: "token/revoke".into(),
                operation: Operation::Revoke,
                reveived_at: None
            }]
        );

        // Sanity test that leases is still empty
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);
    }

    #[tokio::test]
    async fn renew() {
        let clock = TestClock::new();
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
            clock.clone(),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::ZERO).await;

        // Setup system mount
        let mount_config = MountConfig {
            max_lease_ttl: std::time::Duration::from_secs(3600 * 24),
            ..Default::default()
        };
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: mount_config,
            path: "psql/".into(),
        };
        mount_store.create(&me).await.unwrap();

        let renew_ttl = Duration::hours(2);

        let recorder_moved = Arc::clone(&recorder);
        let clock_moved = clock.clone();
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            let clock = clock_moved.clone();
            async move {
                secret_engine_handle(req, recorder, Some(renew_ttl.to_std().unwrap()), clock).await
            }
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

        let ttl = Duration::hours(4);
        let le = LeaseEntry::new(
            me.path.clone(),
            Some("creds".into()),
            &(),
            Some("creds".into()),
            &(),
            clock.now(),
            ttl,
        )
        .unwrap();

        assert!(exp_m.register(le.clone()).await.is_ok());
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);

        // 1 hour before it expires. Lets renew!
        advance_to(&clock, le.expires_at - Duration::hours(1)).await;

        // Renew
        let new_le = exp_m.renew_lease_entry(le.id(), None).await.unwrap();
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![le.clone()]);
        let new_expire_time = new_le.expires_at;

        // Advance 1 hours until the original revocation time.
        advance_to(&clock, le.expires_at).await;

        // Still not revoked
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

        // Advance until the new revocation time.
        advance_to(&clock, new_expire_time).await;

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

        // Sanity test that leases is still empty
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);
    }

    #[tokio::test]
    async fn retry_failed_revocation() {
        let clock = TestClock::new();
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let mut exp_m = ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
            clock.clone(),
        );
        exp_m.revocation_retry_timeout = Duration::milliseconds(10);
        exp_m.revocation_max_retries = 5;
        let exp_m = Arc::new(exp_m);

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::ZERO).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            path: "psql/".into(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let clock_moved = clock.clone();
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            let clock = clock_moved.clone();
            async move { secret_engine_handle(req, recorder, None, clock).await }
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

        let ttl = Duration::hours(4);
        let le = LeaseEntry::new(
            me.path.clone(),
            // This will ensure revocation returns 404 error
            Some("invalid-revoke-path".into()),
            &(),
            Some("creds".into()),
            &(),
            clock.now(),
            ttl,
        )
        .unwrap();

        assert!(exp_m.register(le.clone()).await.is_ok());

        // Wait until revocation time
        advance_to(&clock, le.expires_at).await;

        for _ in 0..exp_m.revocation_max_retries * 2 {
            advance(&clock, exp_m.revocation_retry_timeout).await;
        }

        let requests = recorder.0.read().await;
        assert_eq!(requests.len(), exp_m.revocation_max_retries as usize);
        drop(requests);

        // Lease should be deleted
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases, vec![]);
    }

    #[tokio::test]
    async fn revoke_for_mount() {
        let clock = TestClock::new();
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
            clock.clone(),
        ));

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::ZERO).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            path: "psql/".into(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let clock_moved = clock.clone();
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            let clock = clock_moved.clone();
            async move { secret_engine_handle(req, recorder, None, clock).await }
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

        let ttl = Duration::hours(4);
        let lease_count = 50;

        for _ in 0..lease_count {
            let le = LeaseEntry::new(
                me.path.clone(),
                Some("creds".into()),
                &(),
                Some("creds".into()),
                &(),
                clock.now(),
                ttl,
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

        // Number of leases revoked == number of requests
        let requests = recorder.0.read().await;
        assert_eq!(requests.len(), lease_count);
        drop(requests);

        // Go to revocation time
        advance(&clock, ttl).await;

        // No new requests has been sent
        let requests = recorder.0.read().await;
        assert_eq!(requests.len(), lease_count);
        drop(requests);
    }

    #[tokio::test]
    async fn slow_revoke_endpoint_does_not_halt_other_revocations() {
        let clock = TestClock::new();
        let recorder = Arc::new(RequestRecorder(RwLock::new(Vec::new())));

        let pool = Arc::new(pool().await);
        let lease_store = Arc::new(LeaseStore::new(Arc::clone(&pool)));
        let mount_store = Arc::new(MountStore::new(Arc::clone(&pool)));
        let router = Arc::new(Router::new());
        let mut exp_m = ExpirationManager::new(
            Arc::clone(&router),
            Arc::clone(&lease_store),
            Arc::clone(&mount_store),
            clock.clone(),
        );
        exp_m.revocation_max_retries = 3;
        exp_m.revocation_timeout = std::time::Duration::from_millis(10);
        let exp_m = Arc::new(exp_m);

        let expiration_manager = Arc::clone(&exp_m);
        tokio::spawn(async move {
            expiration_manager.start().await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::ZERO).await;

        // Setup system mount
        let me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            path: "psql/".into(),
        };
        mount_store.create(&me).await.unwrap();

        let recorder_moved = Arc::clone(&recorder);
        let clock_moved = clock.clone();
        let handler = SyncService::new(tower::service_fn(move |req| {
            let recorder = Arc::clone(&recorder_moved);
            let clock = clock_moved.clone();
            async move { secret_engine_handle(req, recorder, None, clock).await }
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

        let ttl = Duration::hours(4);
        let fast_lease_revocation_time = clock.now() + ttl;
        let lease_count = 5;

        for i in 0..lease_count {
            let le = LeaseEntry::new(
                me.path.clone(),
                Some("creds".into()),
                &i,
                Some("creds".into()),
                &i,
                clock.now(),
                ttl,
            )
            .unwrap();
            assert!(exp_m.register(le.clone()).await.is_ok());
        }

        // Register lease that will be slow to revoke
        let slow_lease = LeaseEntry::new(
            me.path.clone(),
            Some("creds-slow".into()),
            &(),
            Some("creds".into()),
            &(),
            clock.now(),
            ttl - Duration::milliseconds(2),
        )
        .unwrap();
        assert!(exp_m.register(slow_lease.clone()).await.is_ok());

        advance_to(&clock, slow_lease.expires_at).await;

        // All leases still stored
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases.len(), lease_count + 1);

        // Advance to time where fast leases will be revoked
        advance_to(&clock, fast_lease_revocation_time).await;

        // All fast lease revocations are gone now
        let leases = lease_store.list().await.unwrap();
        assert_eq!(leases.len(), 1);
    }
}
