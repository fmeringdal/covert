use std::{
    ops::Deref,
    task::{Context, Poll},
};

use covert_types::{error::ApiError, request::Request};
use tower::Service;

use super::FromRequest;

#[derive(Debug, Clone, Copy, Default)]
pub struct Extension<T>(pub T);

impl<T> Deref for Extension<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S, T> tower::Layer<S> for Extension<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Service = AddExtension<S, T>;

    fn layer(&self, inner: S) -> Self::Service {
        AddExtension {
            inner,
            value: self.0.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AddExtension<S, T> {
    pub(crate) inner: S,
    pub(crate) value: T,
}

impl<S, T> Service<Request> for AddExtension<S, T>
where
    S: Service<Request>,
    T: Clone + Send + Sync + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        req.extensions.insert(self.value.clone());
        self.inner.call(req)
    }
}

impl<T> FromRequest for Extension<T>
where
    T: Clone + Send + Sync + 'static,
{
    #[tracing::instrument(level = "debug", name = "extension_extractor", skip_all)]
    fn from_request(req: &mut Request) -> Result<Self, ApiError> {
        req.extensions
            .get::<T>()
            .ok_or_else(ApiError::internal_error)
            .map(|ext| Extension(ext.clone()))
    }
}
