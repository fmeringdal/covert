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
    response::ResponseWithCtx,
    store::{
        identity_store::IdentityStore,
        token_store::{TokenEntry, TokenStore},
    },
    system::RevokeTokenParams,
    ExpirationManager, LeaseEntry,
};

#[derive(Clone)]
pub struct LeaseRegistrationService<S> {
    inner: S,
    expiration_manager: Arc<ExpirationManager>,
    token_store: Arc<TokenStore>,
    identity_store: Arc<IdentityStore>,
}

impl<S> LeaseRegistrationService<S> {
    pub fn new(
        inner: S,
        expiration_manager: Arc<ExpirationManager>,
        token_store: Arc<TokenStore>,
        identity_store: Arc<IdentityStore>,
    ) -> Self {
        Self {
            inner,
            expiration_manager,
            token_store,
            identity_store,
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
            let resp = this.inner.call(req).await?;
            let backend_mount_path = &resp.ctx.backend_mount_path;
            let backend_config = &resp.ctx.backend_config;

            match resp.response {
                Response::Lease(lease) => {
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
                    let alias = EntityAlias {
                        name: auth.alias.clone(),
                        mount_path: backend_mount_path.clone(),
                    };
                    let entity = this.identity_store.get_entity_from_alias(&alias).await?;
                    match entity {
                        Some(entity) => {
                            let now = Utc::now();
                            let issued_at = now;
                            let ttl = calculate_ttl(now, issued_at, backend_config, auth.ttl)
                                .map_err(|_| ApiError::internal_error())?;

                            let token_entry = TokenEntry::new(entity.name().to_string(), ttl);
                            this.token_store.create(&token_entry).await?;
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
    token_store: Arc<TokenStore>,
    identity_store: Arc<IdentityStore>,
}

impl LeaseRegistrationLayer {
    pub fn new(
        expiration_manager: Arc<ExpirationManager>,
        token_store: Arc<TokenStore>,
        identity_store: Arc<IdentityStore>,
    ) -> Self {
        Self {
            expiration_manager,
            token_store,
            identity_store,
        }
    }
}

impl<S> Layer<S> for LeaseRegistrationLayer {
    type Service = LeaseRegistrationService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LeaseRegistrationService::new(
            inner,
            Arc::clone(&self.expiration_manager),
            Arc::clone(&self.token_store),
            Arc::clone(&self.identity_store),
        )
    }
}
