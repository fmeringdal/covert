use std::sync::Arc;

use covert_types::{error::ApiError, request::Request};
use tower::{Layer, Service};

use crate::{response::ResponseWithCtx, Core};

#[derive(Clone)]
pub struct CoreStateInjector<S> {
    core: Arc<Core>,
    inner: S,
}

impl<S> CoreStateInjector<S> {
    pub fn new(inner: S, core: Arc<Core>) -> Self {
        Self { core, inner }
    }
}

impl<S> Service<Request> for CoreStateInjector<S>
where
    S: Service<Request, Response = ResponseWithCtx, Error = ApiError>,
{
    type Response = ResponseWithCtx;

    type Error = S::Error;

    type Future = S::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let state = self.core.state();
        req.extensions.insert(state);
        self.inner.call(req)
    }
}

pub struct CoreStateInjectorLayer {
    core: Arc<Core>,
}

impl CoreStateInjectorLayer {
    pub fn new(core: Arc<Core>) -> Self {
        Self { core }
    }
}

impl<S: Service<Request>> Layer<S> for CoreStateInjectorLayer {
    type Service = CoreStateInjector<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CoreStateInjector::new(inner, Arc::clone(&self.core))
    }
}
