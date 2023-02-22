use covert_types::{error::ApiError, request::Request, state::StorageState};
use futures::future::BoxFuture;
use hyper::StatusCode;
use tower::{Layer, Service};
use tracing_error::SpanTrace;

use crate::{repos::namespace::NamespaceRepo, response::ResponseWithCtx};

#[derive(Clone)]
pub struct NamespaceExtensionService<S> {
    ns_repo: NamespaceRepo,
    inner: S,
}

impl<S> NamespaceExtensionService<S> {
    pub fn new(inner: S, ns_repo: NamespaceRepo) -> Self {
        Self { ns_repo, inner }
    }
}

impl<S> Service<Request> for NamespaceExtensionService<S>
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
            if req.extensions.get::<StorageState>() == Some(&StorageState::Unsealed) {
                let ns = this
                    .ns_repo
                    .find_by_path(&req.namespace)
                    .await?
                    .ok_or_else(|| ApiError {
                        status_code: StatusCode::BAD_REQUEST,
                        span_trace: Some(SpanTrace::capture()),
                        error: anyhow::Error::msg("Invalid namespace"),
                    })?;
                req.extensions.insert(ns);
            }
            this.inner.call(req).await
        })
    }
}

pub struct NamespaceExtensionLayer {
    ns_repo: NamespaceRepo,
}

impl NamespaceExtensionLayer {
    pub fn new(ns_repo: NamespaceRepo) -> Self {
        Self { ns_repo }
    }
}

impl<S: Service<Request>> Layer<S> for NamespaceExtensionLayer {
    type Service = NamespaceExtensionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        NamespaceExtensionService::new(inner, self.ns_repo.clone())
    }
}
