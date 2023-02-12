use std::{future::Future, marker::PhantomData, pin::Pin};

use tower::{util::BoxCloneService, Service};

use super::method_router::RouteConfig;
use super::{extract::FromRequest, method_router::Route};
use covert_types::{error::ApiError, request::Request, response::Response};

#[async_trait::async_trait]
pub trait Handler<T: Send + 'static>: Clone + Send + Sized + 'static {
    /// Call the handler with the given request.
    async fn call(self, req: Request) -> Result<Response, ApiError>;

    fn into_route(self, config: RouteConfig) -> Route {
        let svc = HandlerService::new(self);
        Route::new(BoxCloneService::new(svc), config)
    }
}

#[async_trait::async_trait]
impl<F, Fut> Handler<()> for F
where
    F: FnOnce() -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Result<Response, ApiError>> + Send,
{
    async fn call(self, _req: Request) -> Result<Response, ApiError> {
        self().await
    }
}

#[allow(unused_macros)]
macro_rules! impl_service {
    ( $($ty:ident),* $(,)? ) => {
        #[async_trait::async_trait]
        impl<F, Fut, E, $($ty,)*> Handler<($($ty,)*)> for F
        where
            F: FnOnce($($ty),*) -> Fut + Clone + Send + 'static,
            Fut: Future<Output = Result<Response, E>> + Send,
            ApiError: From<E>,
            $( $ty: FromRequest + Send + 'static,)*
        {
            #[allow(non_snake_case)]
            async fn call(self, mut req: Request) -> Result<Response, ApiError> {
                $(
                    let $ty = $ty::from_request(&mut req)?;
                )*

                self($($ty),*).await.map_err(Into::into)
            }
        }
    }
}

impl_service!(T1);
impl_service!(T1, T2);
impl_service!(T1, T2, T3);
impl_service!(T1, T2, T3, T4);
impl_service!(T1, T2, T3, T4, T5);
impl_service!(T1, T2, T3, T4, T5, T6);
impl_service!(T1, T2, T3, T4, T5, T6, T7);
impl_service!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_service!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_service!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_service!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_service!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
impl_service!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13);
impl_service!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14);
impl_service!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15);
impl_service!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16);

struct HandlerService<H, T> {
    handler: H,
    _marker: PhantomData<T>,
}

impl<H, T> HandlerService<H, T> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            _marker: PhantomData,
        }
    }
}

impl<H: Clone, T> Clone for HandlerService<H, T> {
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            _marker: PhantomData,
        }
    }
}

impl<H, T> Service<Request> for HandlerService<H, T>
where
    H: Handler<T>,
    T: Send + 'static,
{
    type Response = Response;

    type Error = ApiError;

    type Future = Pin<Box<dyn Future<Output = Result<Response, ApiError>> + Send + 'static>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let handler = self.handler.clone();
        Box::pin(async move { handler.call(req).await })
    }
}
