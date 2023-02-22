use std::str::FromStr;

use covert_types::{
    auth::AuthPolicy, error::ApiError, request::Request, state::StorageState, token::Token,
};
use futures::future::BoxFuture;
use tower::{Layer, Service};
use tracing::error;

use crate::{
    repos::{namespace::NamespaceRepo, token::TokenRepo},
    response::ResponseWithCtx,
};

#[derive(Clone)]
pub struct AuthService<S: Service<Request>> {
    inner: S,
    token_repo: TokenRepo,
    namespace_repo: NamespaceRepo,
}

impl<S: Service<Request>> AuthService<S> {
    pub fn new(inner: S, token_repo: TokenRepo, namespace_repo: NamespaceRepo) -> Self {
        Self {
            inner,
            token_repo,
            namespace_repo,
        }
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
            let authorized = is_authorized(&req, &this.token_repo, &this.namespace_repo).await?;
            if authorized {
                req.extensions.insert(AuthPolicy::Authenticated);
            } else {
                req.extensions.insert(AuthPolicy::Unauthenticated);
            }

            this.inner.call(req).await
        })
    }
}

pub struct AuthServiceLayer {
    token_repo: TokenRepo,
    namespace_repo: NamespaceRepo,
}

impl AuthServiceLayer {
    pub fn new(token_repo: TokenRepo, namespace_repo: NamespaceRepo) -> Self {
        Self {
            token_repo,
            namespace_repo,
        }
    }
}

impl<S: Service<Request>> Layer<S> for AuthServiceLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService::new(inner, self.token_repo.clone(), self.namespace_repo.clone())
    }
}

async fn is_authorized(
    req: &Request,
    token_repo: &TokenRepo,
    namespace_repo: &NamespaceRepo,
) -> Result<bool, ApiError> {
    if req.extensions.get::<StorageState>() != Some(&StorageState::Unsealed) {
        return Ok(false);
    }

    let Some(token) = req.token.as_ref() else {
        return Ok(false);
    };
    let token = Token::from_str(token)?;
    let mut policies = token_repo.lookup_policies(&token).await?;

    let Some(policy_namespace_id) = policies.get(0).map(|p| &p.namespace_id).cloned() else {
        return Ok(false);
    };
    let policy_namespace_prefix = namespace_repo.get_full_path(&policy_namespace_id).await?;

    let namespace_prefix = req.namespace.join("/");
    let path = format!("{}/{}", namespace_prefix, req.path);

    let is_authorized = policies.iter_mut().any(|policy| {
        // This should never happen, but it is a nice extra safeguard.
        if policy.namespace_id != policy_namespace_id {
            error!("Token had attached policies from different namespaces");
            return false;
        }

        // Attach the namespace prefix to the policy paths from where the
        // namespace they were created in.
        for path in &mut policy.paths {
            let maybe_slash = if path.path.starts_with('/') { "" } else { "/" };
            path.path = format!("{policy_namespace_prefix}{maybe_slash}{}", path.path);
        }
        policy.is_authorized(&path, &[req.operation])
    });

    Ok(is_authorized)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use bytes::Bytes;
    use chrono::{Duration, Utc};
    use covert_types::{
        entity::Entity,
        policy::{PathPolicy, Policy},
        request::Operation,
    };
    use hyper::http::Extensions;
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::repos::{mount::tests::pool, namespace::Namespace, token::TokenEntry, Repos};

    use super::*;

    #[tokio::test]
    async fn authorizes_request_with_valid_token() {
        let pool = Arc::new(pool().await);
        let u_pool = SqlitePool::connect(":memory:").await.unwrap();
        let repos = Repos::new(pool, u_pool);

        // Setup root namespace
        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        repos.namespace.create(&ns).await.unwrap();

        // Create entity and policy
        let entity = Entity {
            name: "foo".to_string(),
            disabled: false,
            namespace_id: ns.id.clone(),
        };
        repos.entity.create(&entity).await.unwrap();

        let policy = Policy {
            name: "foo-policy".to_string(),
            paths: vec![PathPolicy {
                path: "*".to_string(),
                operations: vec![Operation::Create],
            }],
            namespace_id: ns.id.clone(),
        };
        repos.policy.create(&policy).await.unwrap();
        repos
            .entity
            .attach_policy(&entity.name, &policy.name, &ns.id)
            .await
            .unwrap();

        // Create token for entity
        let token = TokenEntry {
            id: Token::new(),
            entity_name: entity.name.clone(),
            expires_at: None,
            issued_at: Utc::now(),
            namespace_id: ns.id.clone(),
        };
        repos.token.create(&token).await.unwrap();

        // Unsealed and we can authenticate
        let mut req = Request {
            id: Uuid::default(),
            operation: Operation::Create,
            namespace: vec![ns.name.clone()],
            path: String::default(),
            data: Bytes::default(),
            extensions: Extensions::default(),
            token: Some(token.id.to_string()),
            params: Vec::default(),
            query_string: String::default(),
            headers: HashMap::default(),
        };
        req.extensions.insert(StorageState::Unsealed);
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(authorized);
    }

    #[tokio::test]
    async fn rejects_request_with_exipired_token() {
        let pool = Arc::new(pool().await);
        let u_pool = SqlitePool::connect(":memory:").await.unwrap();
        let repos = Repos::new(pool, u_pool);

        // Setup root namespace
        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        repos.namespace.create(&ns).await.unwrap();

        // Create entity and policy
        let entity = Entity {
            name: "foo".to_string(),
            disabled: false,
            namespace_id: ns.id.clone(),
        };
        repos.entity.create(&entity).await.unwrap();

        let policy = Policy {
            name: "foo-policy".to_string(),
            paths: vec![PathPolicy {
                path: "*".to_string(),
                operations: vec![Operation::Create],
            }],
            namespace_id: ns.id.clone(),
        };
        repos.policy.create(&policy).await.unwrap();
        repos
            .entity
            .attach_policy(&entity.name, &policy.name, &ns.id)
            .await
            .unwrap();

        // Create token for entity
        let token = TokenEntry {
            id: Token::new(),
            entity_name: entity.name.clone(),
            expires_at: Some(Utc::now() - Duration::hours(1)),
            issued_at: Utc::now() - Duration::hours(2),
            namespace_id: ns.id.clone(),
        };
        repos.token.create(&token).await.unwrap();

        // Unsealed and we can authenticate
        let mut req = Request {
            id: Uuid::default(),
            operation: Operation::Create,
            namespace: vec![ns.name.clone()],
            path: String::default(),
            data: Bytes::default(),
            extensions: Extensions::default(),
            token: Some(token.id.to_string()),
            params: Vec::default(),
            query_string: String::default(),
            headers: HashMap::default(),
        };
        req.extensions.insert(StorageState::Unsealed);
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(!authorized);
    }

    #[tokio::test]
    async fn rejects_request_without_token() {
        let pool = Arc::new(pool().await);
        let u_pool = SqlitePool::connect(":memory:").await.unwrap();
        let repos = Repos::new(pool, u_pool);

        // Setup root namespace
        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        repos.namespace.create(&ns).await.unwrap();

        // Unsealed and we can authenticate
        let mut req = Request {
            id: Uuid::default(),
            operation: Operation::Create,
            namespace: vec![ns.name.clone()],
            path: String::default(),
            data: Bytes::default(),
            extensions: Extensions::default(),
            token: None,
            params: Vec::default(),
            query_string: String::default(),
            headers: HashMap::default(),
        };
        req.extensions.insert(StorageState::Unsealed);
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(!authorized);
    }

    #[tokio::test]
    async fn unauthenticated_when_not_unsealed() {
        let pool = Arc::new(pool().await);
        let u_pool = SqlitePool::connect(":memory:").await.unwrap();
        let repos = Repos::new(pool, u_pool);

        // Setup root namespace
        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        repos.namespace.create(&ns).await.unwrap();

        // Create entity and policy
        let entity = Entity {
            name: "foo".to_string(),
            disabled: false,
            namespace_id: ns.id.clone(),
        };
        repos.entity.create(&entity).await.unwrap();

        let policy = Policy {
            name: "foo-policy".to_string(),
            paths: vec![PathPolicy {
                path: "*".to_string(),
                operations: vec![Operation::Create],
            }],
            namespace_id: ns.id.clone(),
        };
        repos.policy.create(&policy).await.unwrap();
        repos
            .entity
            .attach_policy(&entity.name, &policy.name, &ns.id)
            .await
            .unwrap();

        // Create token for entity
        let token = TokenEntry {
            id: Token::new(),
            entity_name: entity.name.clone(),
            expires_at: None,
            issued_at: Utc::now(),
            namespace_id: ns.id.clone(),
        };
        repos.token.create(&token).await.unwrap();

        let mut req = Request {
            id: Uuid::default(),
            operation: Operation::Create,
            namespace: vec![ns.name.clone()],
            path: String::default(),
            data: Bytes::default(),
            extensions: Extensions::default(),
            token: Some(token.id.to_string()),
            params: Vec::default(),
            query_string: String::default(),
            headers: HashMap::default(),
        };
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(!authorized);

        for state in [StorageState::Uninitialized, StorageState::Sealed] {
            req.extensions.insert(state);
            let authorized = is_authorized(&req, &repos.token, &repos.namespace)
                .await
                .unwrap();
            assert!(!authorized);
        }

        // Unsealed and we can authenticate
        req.extensions.insert(StorageState::Unsealed);
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(authorized);
    }

    #[tokio::test]
    async fn child_namespace_token_accessing_root_ns_denied() {
        let pool = Arc::new(pool().await);
        let u_pool = SqlitePool::connect(":memory:").await.unwrap();
        let repos = Repos::new(pool, u_pool);

        // Setup root namespace
        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        repos.namespace.create(&ns).await.unwrap();

        // Setup foo sub namespace
        let foo_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "foo".to_string(),
            parent_namespace_id: Some(ns.id.clone()),
        };
        repos.namespace.create(&foo_ns).await.unwrap();

        // Create entity and policy in foo ns
        let entity = Entity {
            name: "foo".to_string(),
            disabled: false,
            namespace_id: foo_ns.id.clone(),
        };
        repos.entity.create(&entity).await.unwrap();

        let policy = Policy {
            name: "foo-policy".to_string(),
            paths: vec![PathPolicy {
                path: "*".to_string(),
                operations: vec![Operation::Create],
            }],
            namespace_id: foo_ns.id.clone(),
        };
        repos.policy.create(&policy).await.unwrap();
        repos
            .entity
            .attach_policy(&entity.name, &policy.name, &foo_ns.id)
            .await
            .unwrap();

        // Create token for entity
        let token = TokenEntry {
            id: Token::new(),
            entity_name: entity.name.clone(),
            expires_at: None,
            issued_at: Utc::now(),
            namespace_id: foo_ns.id.clone(),
        };
        repos.token.create(&token).await.unwrap();

        // Accessing sys/ in foo namespace works
        let mut req = Request {
            id: Uuid::default(),
            operation: Operation::Create,
            // Trying to access sys/ in foo namespace
            namespace: vec![ns.name.clone(), foo_ns.name.clone()],
            path: "sys/some-path".to_string(),
            data: Bytes::default(),
            extensions: Extensions::default(),
            token: Some(token.id.to_string()),
            params: Vec::default(),
            query_string: String::default(),
            headers: HashMap::default(),
        };
        req.extensions.insert(StorageState::Unsealed);
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(authorized);

        // But accessing sys/ in root namespace does not work
        let mut req = Request {
            id: Uuid::default(),
            operation: Operation::Create,
            namespace: vec![ns.name.clone()],
            path: "sys/some-path".to_string(),
            data: Bytes::default(),
            extensions: Extensions::default(),
            token: Some(token.id.to_string()),
            params: Vec::default(),
            query_string: String::default(),
            headers: HashMap::default(),
        };
        req.extensions.insert(StorageState::Unsealed);
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(!authorized);
    }

    #[tokio::test]
    async fn unauthenticated_when_policy_does_not_allow_request() {
        let pool = Arc::new(pool().await);
        let u_pool = SqlitePool::connect(":memory:").await.unwrap();
        let repos = Repos::new(pool, u_pool);

        // Setup root namespace
        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        repos.namespace.create(&ns).await.unwrap();

        // Create entity and policy in root ns
        let entity = Entity {
            name: "foo".to_string(),
            disabled: false,
            namespace_id: ns.id.clone(),
        };
        repos.entity.create(&entity).await.unwrap();

        let policy = Policy {
            name: "foo-policy".to_string(),
            paths: vec![PathPolicy {
                path: "secrets/marketing/*".to_string(),
                operations: vec![Operation::Create],
            }],
            namespace_id: ns.id.clone(),
        };
        repos.policy.create(&policy).await.unwrap();
        repos
            .entity
            .attach_policy(&entity.name, &policy.name, &ns.id)
            .await
            .unwrap();

        // Create token for entity
        let token = TokenEntry {
            id: Token::new(),
            entity_name: entity.name.clone(),
            expires_at: None,
            issued_at: Utc::now(),
            namespace_id: ns.id.clone(),
        };
        repos.token.create(&token).await.unwrap();

        // Accessing secrets/marketing/* with create is allowed by policy
        let mut req = Request {
            id: Uuid::default(),
            operation: Operation::Create,
            // Trying to access sys/ in foo namespace
            namespace: vec![ns.name.clone()],
            path: "secrets/marketing/some-key".to_string(),
            data: Bytes::default(),
            extensions: Extensions::default(),
            token: Some(token.id.to_string()),
            params: Vec::default(),
            query_string: String::default(),
            headers: HashMap::default(),
        };
        req.extensions.insert(StorageState::Unsealed);
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(authorized);

        // Accessing secrets/marketing/* with read is *not* allowed by policy
        req.operation = Operation::Read;
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(!authorized);

        // Accessing secrets/not-marketing/* with create is *not* allowed by policy
        req.operation = Operation::Create;
        req.path = "secrets/not-marketing/some-key".to_string();
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(!authorized);
    }

    #[tokio::test]
    async fn deny_access_sibling_ns_with_same_prefix_name() {
        let pool = Arc::new(pool().await);
        let u_pool = SqlitePool::connect(":memory:").await.unwrap();
        let repos = Repos::new(pool, u_pool);

        // Setup root namespace
        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        repos.namespace.create(&ns).await.unwrap();

        // Setup foo namespace
        let foo_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "foo".to_string(),
            parent_namespace_id: Some(ns.id.clone()),
        };
        repos.namespace.create(&foo_ns).await.unwrap();

        // Setup f namespace
        let f_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "f".to_string(),
            parent_namespace_id: Some(ns.id.clone()),
        };
        repos.namespace.create(&f_ns).await.unwrap();

        // Create entity and policy in root ns
        let entity = Entity {
            name: "f-root".to_string(),
            disabled: false,
            namespace_id: f_ns.id.clone(),
        };
        repos.entity.create(&entity).await.unwrap();

        let policy = Policy {
            name: "f-root-policy".to_string(),
            paths: vec![PathPolicy {
                path: "*".to_string(),
                operations: vec![Operation::Create],
            }],
            namespace_id: f_ns.id.clone(),
        };
        repos.policy.create(&policy).await.unwrap();
        repos
            .entity
            .attach_policy(&entity.name, &policy.name, &f_ns.id)
            .await
            .unwrap();

        // Create token for entity
        let token = TokenEntry {
            id: Token::new(),
            entity_name: entity.name.clone(),
            expires_at: None,
            issued_at: Utc::now(),
            namespace_id: f_ns.id.clone(),
        };
        repos.token.create(&token).await.unwrap();

        // Is authorized in f_ns
        let mut req = Request {
            id: Uuid::default(),
            operation: Operation::Create,
            namespace: vec![ns.name.clone(), f_ns.name.clone()],
            path: "sys/some-path".to_string(),
            data: Bytes::default(),
            extensions: Extensions::default(),
            token: Some(token.id.to_string()),
            params: Vec::default(),
            query_string: String::default(),
            headers: HashMap::default(),
        };
        req.extensions.insert(StorageState::Unsealed);
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(authorized);

        // Is *not* authorized in foo_ns
        req.namespace = vec![ns.name.clone(), foo_ns.name.clone()];
        let authorized = is_authorized(&req, &repos.token, &repos.namespace)
            .await
            .unwrap();
        assert!(!authorized);
    }
}
