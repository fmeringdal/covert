use std::{collections::HashMap, future::Future, pin::Pin, task::Poll};

use covert_types::auth::AuthPolicy;
use covert_types::error::ApiError;
use covert_types::request::{Operation, Request};
use covert_types::response::Response;
use tower::{util::BoxCloneService, Service};
use tower::{Layer, ServiceExt};

use covert_types::state::VaultState;

use super::handler::Handler;

#[derive(Debug, Clone)]
pub struct Route {
    handler: BoxCloneService<Request, Response, ApiError>,
    config: RouteConfig,
}

#[derive(Debug, Clone)]
pub struct RouteConfig {
    pub policy: AuthPolicy,
    pub state: Vec<VaultState>,
}

impl RouteConfig {
    #[must_use]
    pub fn unauthenticated() -> Self {
        Self {
            policy: AuthPolicy::Unauthenticated,
            ..Default::default()
        }
    }
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            policy: AuthPolicy::Authenticated,
            state: vec![VaultState::Unsealed],
        }
    }
}

impl Route {
    #[must_use]
    pub fn new(handler: BoxCloneService<Request, Response, ApiError>, config: RouteConfig) -> Self {
        Self { handler, config }
    }
}

impl Service<Request> for Route {
    type Response = Response;

    type Error = ApiError;

    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.handler.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let state = req
            .extensions
            .get::<VaultState>()
            .expect("vault should always have a state");
        if !self.config.state.contains(state) {
            let state = *state;
            return Box::pin(async move { Err(ApiError::invalid_state(state)) });
        }

        let Some(policy) = req.extensions.get::<AuthPolicy>() else {
            return Box::pin(async { Err(ApiError::unauthorized()) });
        };
        let auth = match self.config.policy {
            AuthPolicy::Root => *policy == AuthPolicy::Root,
            AuthPolicy::Authenticated => {
                matches!(policy, AuthPolicy::Root | AuthPolicy::Authenticated)
            }
            AuthPolicy::Unauthenticated => true,
        };
        if !auth {
            return Box::pin(async { Err(ApiError::unauthorized()) });
        }

        self.handler.call(req)
    }
}

#[derive(Debug, Clone)]
pub struct MethodRouter {
    routes: HashMap<Operation, Route>,
}

impl Default for MethodRouter {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! chained_handlers {
    ($operation:ident, $method:ident, $method_with_config:ident) => {
        #[must_use]
        pub fn $method<H, T>(mut self, handler: H) -> Self
        where
            H: Handler<T>,
            T: Send + 'static,
        {
            let route = handler.into_route(RouteConfig::default());
            self.routes.insert(Operation::$operation, route);
            self
        }

        #[must_use]
        pub fn $method_with_config<H, T>(mut self, handler: H, config: RouteConfig) -> Self
        where
            H: Handler<T>,
            T: Send + 'static,
        {
            let route = handler.into_route(config);
            self.routes.insert(Operation::$operation, route);
            self
        }
    };
}

macro_rules! top_level_handlers {
    ($operation:ident, $method:ident, $method_with_config:ident) => {
        #[must_use]
        pub fn $method<H, T>(handler: H) -> MethodRouter
        where
            H: Handler<T>,
            T: Send + 'static,
        {
            MethodRouter::new().on(Operation::$operation, handler, RouteConfig::default())
        }

        #[must_use]
        pub fn $method_with_config<H, T>(handler: H, config: RouteConfig) -> MethodRouter
        where
            H: Handler<T>,
            T: Send + 'static,
        {
            MethodRouter::new().on(Operation::$operation, handler, config)
        }
    };
}

top_level_handlers!(Create, create, create_with_config);
top_level_handlers!(Read, read, read_with_config);
top_level_handlers!(Update, update, update_with_config);
top_level_handlers!(Delete, delete, delete_with_config);
top_level_handlers!(Revoke, revoke, revoke_with_config);
top_level_handlers!(Renew, renew, renew_with_config);

impl MethodRouter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            routes: HashMap::default(),
        }
    }

    chained_handlers!(Create, create, create_with_config);
    chained_handlers!(Read, read, read_with_config);
    chained_handlers!(Update, update, update_with_config);
    chained_handlers!(Delete, delete, delete_with_config);
    chained_handlers!(Revoke, revoke, revoke_with_config);
    chained_handlers!(Renew, renew, renew_with_config);

    #[must_use]
    pub fn on<H, T>(mut self, operation: Operation, handler: H, config: RouteConfig) -> Self
    where
        H: Handler<T>,
        T: Send + 'static,
    {
        let route = handler.into_route(config);
        self.routes.insert(operation, route);
        self
    }

    #[must_use]
    pub fn layer<L>(self, layer: L) -> Self
    where
        L: Layer<Route>,
        L::Service:
            Service<Request, Error = ApiError, Response = Response> + Clone + Send + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        let routes = self
            .routes
            .into_iter()
            .map(|(op, route)| {
                let config = route.config.clone();
                let svc = layer.layer(route);
                let svc = BoxCloneService::new(svc);
                let route = Route::new(svc, config);
                (op, route)
            })
            .collect();

        Self { routes }
    }
}

impl Service<Request> for MethodRouter {
    type Response = Response;

    type Error = ApiError;

    type Future = Pin<Box<dyn Future<Output = Result<Response, ApiError>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let route = self.routes.get(&req.operation).map(Clone::clone);

        Box::pin(async move {
            match route {
                Some(route) => route.oneshot(req).await,
                None => Err(ApiError::not_found()),
            }
        })
    }
}
