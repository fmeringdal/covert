use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use covert_storage::EncryptedPool;
use covert_types::{policy::Policy, token::Token};
use serde::{Deserialize, Serialize};

use crate::error::Error;

use super::policy::PolicyRaw;

pub struct TokenRepo {
    pool: Arc<EncryptedPool>,
}

impl Clone for TokenRepo {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
        }
    }
}

impl TokenRepo {
    pub fn new(pool: Arc<EncryptedPool>) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip_all)]
    pub async fn lookup_policies(&self, id: &Token) -> Result<Vec<Policy>, Error> {
        sqlx::query_as(
            "SELECT P.* FROM TOKENS T
            INNER JOIN ENTITIES E ON T.entity_name = E.name AND T.namespace_id = E.namespace_id
            INNER JOIN ENTITY_POLICIES EP ON E.name = EP.entity_name AND E.namespace_id = EP.namespace_id
            INNER JOIN POLICIES P ON EP.policy_name = P.name AND EP.namespace_id = P.namespace_id
            WHERE T.token = ? AND (T.expires_at IS NULL OR T.expires_at > ?)",
        )
        .bind(id.to_string())
        .bind(Utc::now())
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .map(|policies: Vec<PolicyRaw>| {
            policies
                .into_iter()
                // TODO: policies that can't be deserialized should be deleted
                .filter_map(|p| p.try_into().ok())
                .collect()
        })
    }

    #[tracing::instrument(skip_all)]
    pub async fn create(&self, te: &TokenEntry) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO TOKENS (token, issued_at, expires_at, entity_name, namespace_id)
            VALUES (?, ?, ?, ?, ?)",
        )
        .bind(te.id.to_string())
        .bind(te.issued_at)
        .bind(te.expires_at)
        .bind(&te.entity_name)
        .bind(&te.namespace_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .map(|_| ())
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove(&self, id: &Token, namespace_id: &str) -> Result<bool, Error> {
        sqlx::query("DELETE FROM TOKENS WHERE token = ? AND namespace_id = ?")
            .bind(id.to_string())
            .bind(namespace_id)
            .execute(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .map(|res| res.rows_affected() == 1)
    }

    #[tracing::instrument(skip_all)]
    pub async fn renew(
        &self,
        id: &Token,
        namespace_id: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<bool, Error> {
        sqlx::query(
            "UPDATE TOKENS SET
            expires_at = ?
            WHERE token = ? AND namespace_id = ?",
        )
        .bind(expires_at)
        .bind(id.to_string())
        .bind(namespace_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .map(|res| res.rows_affected() == 1)
    }
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct TokenEntry {
    /// ID of this entry
    pub id: Token,
    /// Entity this token belongs to
    pub entity_name: String,
    /// Valid until timestamp
    pub expires_at: Option<DateTime<Utc>>,
    /// Time of issuance
    pub issued_at: DateTime<Utc>,
    /// Namespace
    pub namespace_id: String,
}

impl TokenEntry {
    pub fn new(entity_name: String, ttl: Duration, namespace_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Token::new(),
            entity_name,
            issued_at: now,
            expires_at: Some(now + ttl),
            namespace_id,
        }
    }

    pub fn id(&self) -> &Token {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use covert_types::{entity::Entity, policy::PathPolicy, request::Operation};
    use uuid::Uuid;

    use crate::repos::{
        entity::EntityRepo,
        mount::tests::pool,
        namespace::{Namespace, NamespaceRepo},
        policy::PolicyRepo,
    };

    use super::*;

    #[tokio::test]
    async fn crud() {
        let pool = Arc::new(pool().await);
        let store = TokenRepo::new(Arc::clone(&pool));
        let policy_repo = Arc::new(PolicyRepo::new(Arc::clone(&pool)));
        let entity_repo = EntityRepo::new(Arc::clone(&pool));
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ns_repo.create(&ns).await.unwrap();

        // Create entity "John" with policy "foo" and "bar"
        let foo_policy = Policy::new(
            "foo".into(),
            vec![PathPolicy::new("foo/".into(), vec![Operation::Read])],
            ns.id.clone(),
        );
        policy_repo.create(&foo_policy).await.unwrap();
        let bar_policy = Policy::new(
            "bar".into(),
            vec![PathPolicy::new("bar/".into(), vec![Operation::Update])],
            ns.id.clone(),
        );
        policy_repo.create(&bar_policy).await.unwrap();

        let entity = Entity::new("John".into(), false, ns.id.clone());
        entity_repo.create(&entity).await.unwrap();
        entity_repo
            .attach_policy(entity.name(), foo_policy.name(), &ns.id)
            .await
            .unwrap();
        entity_repo
            .attach_policy(entity.name(), bar_policy.name(), &ns.id)
            .await
            .unwrap();

        // Now create token for "John"
        let token = TokenEntry::new(entity.name().to_string(), Duration::hours(1), ns.id.clone());
        assert!(store.create(&token).await.is_ok());

        // Lookup the attached policies for token
        assert_eq!(
            store.lookup_policies(token.id()).await.unwrap(),
            vec![bar_policy.clone(), foo_policy.clone()]
        );

        // Delete token
        assert!(store.remove(token.id(), &ns.id).await.unwrap());

        // No policies should be returned for token after deletion
        assert!(store.lookup_policies(token.id()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn no_policies_for_expired_token() {
        let pool = Arc::new(pool().await);
        let store = TokenRepo::new(Arc::clone(&pool));
        let policy_repo = Arc::new(PolicyRepo::new(Arc::clone(&pool)));
        let entity_repo = EntityRepo::new(Arc::clone(&pool));
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ns_repo.create(&ns).await.unwrap();

        // Create entity "John" with policy "foo" and "bar"
        let foo_policy = Policy::new(
            "foo".into(),
            vec![PathPolicy::new("foo/".into(), vec![Operation::Read])],
            ns.id.clone(),
        );
        policy_repo.create(&foo_policy).await.unwrap();
        let bar_policy = Policy::new(
            "bar".into(),
            vec![PathPolicy::new("bar/".into(), vec![Operation::Update])],
            ns.id.clone(),
        );
        policy_repo.create(&bar_policy).await.unwrap();

        let entity = Entity::new("John".into(), false, ns.id.clone());
        entity_repo.create(&entity).await.unwrap();
        entity_repo
            .attach_policy(entity.name(), foo_policy.name(), &ns.id)
            .await
            .unwrap();
        entity_repo
            .attach_policy(entity.name(), bar_policy.name(), &ns.id)
            .await
            .unwrap();

        // Now create token for "John"
        let token = TokenEntry::new(entity.name().to_string(), Duration::hours(1), ns.id.clone());
        assert!(store.create(&token).await.is_ok());

        // Lookup the attached policies for token
        assert_eq!(store.lookup_policies(token.id()).await.unwrap().len(), 2);

        // Hack to update the token to be expired
        let update_resp = sqlx::query(
            "UPDATE TOKENS SET
            expires_at = ?
            WHERE token = ?",
        )
        .bind(Utc::now() - Duration::hours(1))
        .bind(token.id().to_string())
        .execute(pool.as_ref())
        .await
        .unwrap();
        assert_eq!(update_resp.rows_affected(), 1);

        // Token no longer has any policies
        assert!(store.lookup_policies(token.id()).await.unwrap().is_empty());
    }
}
