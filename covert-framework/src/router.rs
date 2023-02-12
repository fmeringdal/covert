use std::{future::Future, marker::PhantomData, pin::Pin};

use covert_types::{error::ApiError, request::Request, response::Response};
use tower::{Layer, Service, ServiceExt};

use super::{
    method_router::{MethodRouter, Route},
    SyncService,
};

pub struct Building;
pub struct Ready;

/// Wrapper around `matchit::Router`
pub struct Router<Stage = Building> {
    routes: Vec<(&'static str, MethodRouter)>,
    router: matchit::Router<MethodRouter>,
    _marker: PhantomData<Stage>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    #[must_use]
    pub fn new() -> Self {
        Self {
            routes: Vec::default(),
            router: matchit::Router::default(),
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn route(mut self, path: &'static str, route: MethodRouter) -> Self {
        self.routes.push((path, route));
        self
    }

    #[must_use]
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: Layer<Route>,
        L::Service:
            Service<Request, Error = ApiError, Response = Response> + Clone + Send + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        self.routes = self
            .routes
            .into_iter()
            .map(|(path, route)| (path, route.layer(&layer)))
            .collect();
        self
    }

    pub fn build(mut self) -> Router<Ready> {
        for (path, route) in self.routes.clone() {
            self.router
                .insert(path, route)
                .expect("No path should overlap");
        }
        Router::<Ready> {
            routes: self.routes,
            router: self.router,
            _marker: PhantomData,
        }
    }
}

impl Router<Ready> {
    // TODO: rename to `into_make_service`?
    pub fn into_service(self) -> SyncService<Request, Response> {
        SyncService::new(self)
    }
}

impl Clone for Router<Ready> {
    fn clone(&self) -> Self {
        Self {
            routes: self.routes.clone(),
            router: self.router.clone(),
            _marker: PhantomData,
        }
    }
}

impl Service<Request> for Router<Ready> {
    type Response = Response;

    type Error = ApiError;

    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let prefixed_path = if req.path.starts_with('/') {
            req.path.clone()
        } else {
            format!("/{}", req.path)
        };
        let matched_router = match self.router.at(&prefixed_path) {
            Ok(r) => r,
            Err(_) => return Box::pin(async { Err(ApiError::not_found()) }),
        };
        req.params = matched_router
            .params
            .iter()
            .map(|(_key, val)| val.to_string())
            .collect();
        let matched_router = matched_router.value.clone();
        Box::pin(async move { matched_router.oneshot(req).await })
    }
}
