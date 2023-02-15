use std::sync::Arc;

use covert_storage::EncryptedPool;
use covert_types::{error::ApiError, request::Request};
use tower::{Layer, Service};

use crate::response::ResponseWithCtx;

#[derive(Clone)]
pub struct CoreStateInjector<S> {
    storage_pool: Arc<EncryptedPool>,
    inner: S,
}

impl<S> CoreStateInjector<S> {
    pub fn new(inner: S, storage_pool: Arc<EncryptedPool>) -> Self {
        Self {
            storage_pool,
            inner,
        }
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
        let state = self.storage_pool.state();
        req.extensions.insert(state);
        self.inner.call(req)
    }
}

pub struct CoreStateInjectorLayer {
    storage_pool: Arc<EncryptedPool>,
}

impl CoreStateInjectorLayer {
    pub fn new(storage_pool: Arc<EncryptedPool>) -> Self {
        Self { storage_pool }
    }
}

impl<S: Service<Request>> Layer<S> for CoreStateInjectorLayer {
    type Service = CoreStateInjector<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CoreStateInjector::new(inner, Arc::clone(&self.storage_pool))
    }
}
