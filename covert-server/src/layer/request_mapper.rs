use std::convert::Infallible;

use covert_types::{error::ApiError, request::Request};
use futures::future::BoxFuture;
use http_body::Limited;
use hyper::{http, Body};
use tower::{Layer, Service, ServiceExt};

use crate::response::ResponseWithCtx;

#[derive(Debug, Clone)]
pub struct LogicalRequestResponseService<S> {
    inner: S,
}

impl<S> LogicalRequestResponseService<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Service<http::Request<Limited<Body>>> for LogicalRequestResponseService<S>
where
    S: Service<Request, Response = ResponseWithCtx, Error = ApiError> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = http::Response<Body>;

    type Error = Infallible;

    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Limited<Body>>) -> Self::Future {
        let this = self.clone();
        Box::pin(async move {
            let logical_req = match Request::new(req).await {
                Ok(req) => req,
                Err(e) => return Ok(e.into()),
            };
            match this.inner.oneshot(logical_req).await {
                Ok(resp) => Ok(resp.into()),
                Err(error) => {
                    let error_report = error.report();
                    tracing::error!(?error_report, "API error encountered");
                    Ok(error.into())
                }
            }
        })
    }
}

pub struct LogicalRequestResponseLayer {}

impl LogicalRequestResponseLayer {
    pub fn new() -> Self {
        Self {}
    }
}

impl<S> Layer<S> for LogicalRequestResponseLayer {
    type Service = LogicalRequestResponseService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LogicalRequestResponseService::new(inner)
    }
}
