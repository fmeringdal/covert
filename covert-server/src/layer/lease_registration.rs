use std::sync::Arc;

use chrono::Utc;
use covert_types::{
    entity::EntityAlias,
    error::ApiError,
    methods::{AuthResponse, SecretLeaseResponse},
    request::Request,
    response::Response,
    ttl::calculate_ttl,
};
use futures::future::BoxFuture;
use tower::{Layer, Service};

use crate::{
    error::{Error, ErrorType},
    repos::{
        entity::EntityRepo,
        namespace::Namespace,
        token::{TokenEntry, TokenRepo},
    },
    response::ResponseWithCtx,
    system::RevokeTokenParams,
    ExpirationManager, LeaseEntry,
};

#[derive(Clone)]
pub struct LeaseRegistrationService<S> {
    inner: S,
    expiration_manager: Arc<ExpirationManager>,
    token_repo: TokenRepo,
    entity_repo: EntityRepo,
}

impl<S> LeaseRegistrationService<S> {
    pub fn new(
        inner: S,
        expiration_manager: Arc<ExpirationManager>,
        token_repo: TokenRepo,
        entity_repo: EntityRepo,
    ) -> Self {
        Self {
            inner,
            expiration_manager,
            token_repo,
            entity_repo,
        }
    }
}

impl<S> Service<Request> for LeaseRegistrationService<S>
where
    S: Service<Request, Response = ResponseWithCtx, Error = ApiError> + Send + Clone + 'static,
    S::Future: Send,
{
    type Response = ResponseWithCtx;

    type Error = ApiError;

    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let mut this = self.clone();
        Box::pin(async move {
            let ns = req.extensions.get::<Namespace>().cloned();

            let resp = this.inner.call(req).await?;
            let backend_mount_path = &resp.ctx.backend_mount_path;
            let backend_config = &resp.ctx.backend_config;

            match resp.response {
                Response::Lease(lease) => {
                    let ns = ns.ok_or_else(ApiError::internal_error)?;

                    let now = Utc::now();
                    let issued_at = now;
                    let ttl = calculate_ttl(now, issued_at, backend_config, lease.ttl)
                        .map_err(|_| ApiError::internal_error())?;

                    let le = LeaseEntry::new(
                        backend_mount_path.clone(),
                        Some(lease.revoke.path),
                        &lease.revoke.data,
                        Some(lease.renew.path),
                        &lease.renew.data,
                        issued_at,
                        ttl,
                        ns.id.clone(),
                    )?;
                    let lease_id = le.id().to_string();
                    this.expiration_manager.register(le).await?;

                    let data = SecretLeaseResponse {
                        data: lease.data,
                        lease_id,
                        ttl: ttl.to_std().map_err(|_| ApiError::internal_error())?,
                    };
                    let data = serde_json::to_value(&data)
                        .map_err(|err| Error::from(ErrorType::BadResponseData(err)))?;

                    Ok(ResponseWithCtx {
                        response: Response::Raw(data),
                        ctx: resp.ctx,
                    })
                }
                Response::Auth(auth) => {
                    let ns = ns.ok_or_else(ApiError::internal_error)?;

                    let alias = EntityAlias {
                        name: auth.alias.clone(),
                        mount_path: backend_mount_path.clone(),
                    };
                    let entity = this
                        .entity_repo
                        .get_entity_from_alias(&alias, &ns.id)
                        .await?;
                    match entity {
                        Some(entity) => {
                            let now = Utc::now();
                            let issued_at = now;
                            let ttl = calculate_ttl(now, issued_at, backend_config, auth.ttl)
                                .map_err(|_| ApiError::internal_error())?;

                            let token_entry =
                                TokenEntry::new(entity.name().to_string(), ttl, ns.id.clone());
                            this.token_repo.create(&token_entry).await?;
                            let token = token_entry.id();

                            let revoke_data = RevokeTokenParams {
                                token: token.clone(),
                            };
                            // TODO: renew token endpoint not implemented yet
                            let renew_data = RevokeTokenParams {
                                token: token.clone(),
                            };
                            let lease = LeaseEntry::new(
                                backend_mount_path.clone(),
                                None,
                                &revoke_data,
                                None,
                                &renew_data,
                                issued_at,
                                ttl,
                                ns.id.clone(),
                            )?;
                            let lease_id = lease.id().to_string();
                            this.expiration_manager.register(lease).await?;

                            let data = AuthResponse {
                                token: token.clone(),
                                lease_id,
                                ttl: ttl.to_std().map_err(|_| ApiError::internal_error())?,
                            };
                            let data = serde_json::to_value(&data)
                                .map_err(|err| Error::from(ErrorType::BadResponseData(err)))?;

                            Ok(ResponseWithCtx {
                                response: Response::Raw(data),
                                ctx: resp.ctx,
                            })
                        }
                        None => Err(ApiError::bad_request()),
                    }
                }
                // Just passthrough the raw data
                Response::Raw(data) => Ok(ResponseWithCtx {
                    response: Response::Raw(data),
                    ctx: resp.ctx,
                }),
            }
        })
    }
}

pub struct LeaseRegistrationLayer {
    expiration_manager: Arc<ExpirationManager>,
    token_repo: TokenRepo,
    entity_repo: EntityRepo,
}

impl LeaseRegistrationLayer {
    pub fn new(
        expiration_manager: Arc<ExpirationManager>,
        token_repo: TokenRepo,
        entity_repo: EntityRepo,
    ) -> Self {
        Self {
            expiration_manager,
            token_repo,
            entity_repo,
        }
    }
}

impl<S> Layer<S> for LeaseRegistrationLayer {
    type Service = LeaseRegistrationService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LeaseRegistrationService::new(
            inner,
            Arc::clone(&self.expiration_manager),
            self.token_repo.clone(),
            self.entity_repo.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bytes::Bytes;
    use covert_sdk::{
        mounts::{BackendType, MountConfig},
        psql::CreateRoleCredsResponse,
    };
    use covert_types::{
        entity::Entity,
        mount::MountEntry,
        policy::{PathPolicy, Policy},
        psql::RoleCredentials,
        request::Operation,
        response::{LeaseRenewRevokeEndpoint, LeaseResponse},
    };
    use hyper::http::Extensions;
    use serde_json::Value;
    use sqlx::SqlitePool;
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::{
        expiration_manager::clock::test::TestClock,
        repos::{mount::tests::pool, Repos},
        response::ResponseContext,
        Router,
    };

    use super::*;

    #[allow(clippy::unused_async)]
    async fn handler(req: Request) -> Result<ResponseWithCtx, ApiError> {
        let response = match &req.headers["response-type"][..] {
            "lease" => Response::Lease(LeaseResponse {
                data: serde_json::to_value(RoleCredentials {
                    username: "foo".to_string(),
                    password: "bar".to_string(),
                })
                .unwrap(),
                renew: LeaseRenewRevokeEndpoint {
                    data: Value::Null,
                    path: "renew".into(),
                },
                revoke: LeaseRenewRevokeEndpoint {
                    data: Value::Null,
                    path: "revoke".into(),
                },
                ttl: None,
            }),
            "auth" => Response::Auth(covert_types::response::AuthResponse {
                alias: "foo".to_string(),
                ttl: None,
            }),
            _ => panic!("Invalid response type"),
        };
        Ok(ResponseWithCtx {
            response,
            ctx: ResponseContext {
                backend_config: MountConfig::default(),
                backend_mount_path: req.headers["mount-path"].to_string(),
            },
        })
    }

    #[tokio::test]
    async fn register_lease_for_lease_responses() {
        let clock = TestClock::new();

        let pool = Arc::new(pool().await);
        let u_pool = SqlitePool::connect(":memory:").await.unwrap();
        let repos = Repos::new(pool, u_pool);

        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        repos.namespace.create(&ns).await.unwrap();

        let router = Arc::new(Router::new(repos.mount.clone()));
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            repos.clone(),
            clock.clone(),
        ));

        let mount = MountEntry {
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            id: Uuid::new_v4(),
            path: "psql/".to_string(),
            namespace_id: ns.id.clone(),
        };
        repos.mount.create(&mount).await.unwrap();

        let inner_handler = tower::service_fn(handler);
        let svc = LeaseRegistrationService::new(inner_handler, exp_m, repos.token, repos.entity);

        let mut headers = HashMap::new();
        headers.insert("response-type".to_string(), "lease".to_string());
        headers.insert("mount-path".to_string(), mount.path.to_string());

        let mut extensions = Extensions::default();
        extensions.insert(ns.clone());

        let req = Request {
            id: Uuid::new_v4(),
            namespace: vec!["root".to_string()],
            data: Bytes::default(),
            extensions,
            headers,
            operation: Operation::Read,
            params: Vec::default(),
            path: String::default(),
            query_string: String::default(),
            token: None,
        };
        let resp = svc.oneshot(req).await.unwrap();

        let lease_resp = resp.response.data::<CreateRoleCredsResponse>().unwrap();
        assert_eq!(lease_resp.ttl, mount.config.default_lease_ttl);
        assert_eq!(lease_resp.data.username, "foo");
        assert_eq!(lease_resp.data.password, "bar");

        // Lookup lease
        let lease = repos
            .lease
            .lookup(&lease_resp.lease_id, &ns.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lease.issued_mount_path, mount.path);
    }

    #[tokio::test]
    async fn register_lease_for_auth_responses() {
        let clock = TestClock::new();

        let pool = Arc::new(pool().await);
        let u_pool = SqlitePool::connect(":memory:").await.unwrap();
        let repos = Repos::new(pool, u_pool);

        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        repos.namespace.create(&ns).await.unwrap();

        let router = Arc::new(Router::new(repos.mount.clone()));
        let exp_m = Arc::new(ExpirationManager::new(
            Arc::clone(&router),
            repos.clone(),
            clock.clone(),
        ));

        let mount = MountEntry {
            backend_type: BackendType::Userpass,
            config: MountConfig::default(),
            id: Uuid::new_v4(),
            path: "auth/userpass/".to_string(),
            namespace_id: ns.id.clone(),
        };
        repos.mount.create(&mount).await.unwrap();

        let entity = Entity::new("foo".to_string(), ns.id.clone());
        repos.entity.create(&entity).await.unwrap();
        repos
            .entity
            .attach_alias(
                &entity.name,
                &EntityAlias {
                    name: "foo".to_string(),
                    mount_path: mount.path.clone(),
                },
                &ns.id,
            )
            .await
            .unwrap();

        let policy = Policy::new(
            "default".to_string(),
            vec![PathPolicy {
                path: "secrets/marketing/".to_string(),
                operations: vec![Operation::Read],
            }],
            ns.id.clone(),
        );
        repos.policy.create(&policy).await.unwrap();
        repos
            .entity
            .attach_policy(&entity.name, &policy.name, &ns.id)
            .await
            .unwrap();

        let inner_handler = tower::service_fn(handler);
        let svc =
            LeaseRegistrationService::new(inner_handler, exp_m, repos.token.clone(), repos.entity);

        let mut headers = HashMap::new();
        headers.insert("response-type".to_string(), "auth".to_string());
        headers.insert("mount-path".to_string(), mount.path.to_string());

        let mut extensions = Extensions::default();
        extensions.insert(ns.clone());

        let req = Request {
            id: Uuid::new_v4(),
            namespace: vec!["root".to_string()],
            data: Bytes::default(),
            extensions,
            headers,
            operation: Operation::Read,
            params: Vec::default(),
            path: String::default(),
            query_string: String::default(),
            token: None,
        };
        let resp = svc.oneshot(req).await.unwrap();

        let auth_resp = resp.response.data::<AuthResponse>().unwrap();
        assert_eq!(auth_resp.ttl, mount.config.default_lease_ttl);

        // Lookup lease
        let lease = repos
            .lease
            .lookup(&auth_resp.lease_id, &ns.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lease.issued_mount_path, mount.path);

        // Check that token was created
        assert_eq!(
            repos
                .token
                .lookup_policies(&auth_resp.token)
                .await
                .unwrap()
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>(),
            vec![policy.name]
        );
    }
}
