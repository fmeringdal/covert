use std::{str::FromStr, sync::Arc};

use covert_framework::extract::FromRequest;
use covert_types::{
    auth::AuthPolicy, error::ApiError, policy::Policy, request::Request, state::VaultState,
    token::Token,
};
use futures::future::BoxFuture;
use tower::{Layer, Service};

use crate::{response::ResponseWithCtx, store::token_store::TokenStore};

#[derive(Debug, Clone)]
pub enum Permissions {
    Root,
    Authenticated(Vec<Policy>),
    Unauthenticated,
}

impl FromRequest for Permissions {
    fn from_request(req: &mut Request) -> Result<Self, ApiError> {
        req.extensions
            .get::<Permissions>()
            .map(Clone::clone)
            .ok_or_else(ApiError::unauthorized)
    }
}

#[derive(Clone)]
pub struct AuthService<S: Service<Request>> {
    inner: S,
    token_store: Arc<TokenStore>,
}

impl<S: Service<Request>> AuthService<S> {
    pub fn new(inner: S, token_store: Arc<TokenStore>) -> Self {
        Self { inner, token_store }
    }
}

impl<S> Service<Request> for AuthService<S>
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

    fn call(&mut self, mut req: Request) -> Self::Future {
        let mut this = self.clone();
        Box::pin(async move {
            // Default policy
            req.extensions.insert(AuthPolicy::Unauthenticated);
            req.extensions.insert(Permissions::Unauthenticated);

            if req.extensions.get::<VaultState>() == Some(&VaultState::Unsealed) {
                if let Some(token) = req.token.as_ref() {
                    let token = Token::from_str(token)?;
                    let policies = this.token_store.lookup_policies(&token).await?;
                    if policies.iter().any(|p| p.name() == "root") {
                        req.extensions.insert(AuthPolicy::Root);
                        req.extensions.insert(Permissions::Root);
                    } else {
                        let is_authorized = policies
                            .iter()
                            .any(|policy| policy.is_authorized(&req.path, &[req.operation]));
                        if is_authorized {
                            req.extensions.insert(AuthPolicy::Authenticated);
                            req.extensions.insert(Permissions::Authenticated(policies));
                        }
                    }
                } else if req.is_sudo {
                    req.extensions.insert(AuthPolicy::Root);
                    req.extensions.insert(Permissions::Root);
                }

                // TODO
                if true {
                    req.extensions.insert(AuthPolicy::Root);
                    req.extensions.insert(Permissions::Root);
                }
            }

            this.inner.call(req).await
        })
    }
}

pub struct AuthServiceLayer {
    token_store: Arc<TokenStore>,
}

impl AuthServiceLayer {
    pub fn new(token_store: Arc<TokenStore>) -> Self {
        Self { token_store }
    }
}

impl<S: Service<Request>> Layer<S> for AuthServiceLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService::new(inner, Arc::clone(&self.token_store))
    }
}
