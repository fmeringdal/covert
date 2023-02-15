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
            "SELECT P.name, P.policy FROM TOKENS T
            INNER JOIN ENTITIES E ON T.entity_name = E.name
            INNER JOIN ENTITY_POLICIES EP ON E.name = EP.entity_name
            INNER JOIN POLICIES P ON EP.policy_name = P.name
            WHERE T.token = ? AND T.expires_at > ?",
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
            "INSERT INTO TOKENS (token, issued_at, expires_at, entity_name)
            VALUES (?, ?, ?, ?)",
        )
        .bind(te.id.to_string())
        .bind(te.issued_at)
        .bind(te.expires_at)
        .bind(&te.entity_name)
        .execute(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .map(|_| ())
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove(&self, id: &Token) -> Result<bool, Error> {
        sqlx::query("DELETE FROM TOKENS WHERE token = ?")
            .bind(id.to_string())
            .execute(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .map(|res| res.rows_affected() == 1)
    }
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct TokenEntry {
    /// ID of this entry
    id: Token,
    /// Entity this token belongs to
    entity_name: String,
    /// Valid until timestamp
    expires_at: Option<DateTime<Utc>>,
    /// Time of issuance
    issued_at: DateTime<Utc>,
}

impl TokenEntry {
    pub fn new(entity_name: String, ttl: Duration) -> Self {
        let now = Utc::now();
        Self {
            id: Token::new(),
            entity_name,
            issued_at: now,
            expires_at: Some(now + ttl),
        }
    }

    pub fn new_root() -> Self {
        Self {
            id: Token::new(),
            entity_name: "root".into(),
            expires_at: None,
            issued_at: Utc::now(),
        }
    }

    pub fn id(&self) -> &Token {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use covert_types::{entity::Entity, policy::PathPolicy, request::Operation};

    use crate::repos::{entity::EntityRepo, mount::tests::pool, policy::PolicyRepo};

    use super::*;

    #[tokio::test]
    async fn crud() {
        let pool = Arc::new(pool().await);
        let store = TokenRepo::new(Arc::clone(&pool));
        let policy_repo = Arc::new(PolicyRepo::new(Arc::clone(&pool)));
        let entity_repo = EntityRepo::new(Arc::clone(&pool));

        // Create entity "John" with policy "foo" and "bar"
        let foo_policy = Policy::new(
            "foo".into(),
            vec![PathPolicy::new("foo/".into(), vec![Operation::Read])],
        );
        policy_repo.create(&foo_policy).await.unwrap();
        let bar_policy = Policy::new(
            "bar".into(),
            vec![PathPolicy::new("bar/".into(), vec![Operation::Update])],
        );
        policy_repo.create(&bar_policy).await.unwrap();

        let entity = Entity::new("John".into(), false);
        entity_repo.create(&entity).await.unwrap();
        entity_repo
            .attach_policy(entity.name(), foo_policy.name())
            .await
            .unwrap();
        entity_repo
            .attach_policy(entity.name(), bar_policy.name())
            .await
            .unwrap();

        // Now create token for "John"
        let token = TokenEntry::new(entity.name().to_string(), Duration::hours(1));
        assert!(store.create(&token).await.is_ok());

        // Lookup the attached policies for token
        assert_eq!(
            store.lookup_policies(token.id()).await.unwrap(),
            vec![foo_policy.clone(), bar_policy.clone()]
        );

        // Delete token
        assert!(store.remove(token.id()).await.unwrap());

        // No policies should be returned for token after deletion
        assert!(store.lookup_policies(token.id()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn no_policies_for_expired_token() {
        let pool = Arc::new(pool().await);
        let store = TokenRepo::new(Arc::clone(&pool));
        let policy_repo = Arc::new(PolicyRepo::new(Arc::clone(&pool)));
        let entity_repo = EntityRepo::new(Arc::clone(&pool));

        // Create entity "John" with policy "foo" and "bar"
        let foo_policy = Policy::new(
            "foo".into(),
            vec![PathPolicy::new("foo/".into(), vec![Operation::Read])],
        );
        policy_repo.create(&foo_policy).await.unwrap();
        let bar_policy = Policy::new(
            "bar".into(),
            vec![PathPolicy::new("bar/".into(), vec![Operation::Update])],
        );
        policy_repo.create(&bar_policy).await.unwrap();

        let entity = Entity::new("John".into(), false);
        entity_repo.create(&entity).await.unwrap();
        entity_repo
            .attach_policy(entity.name(), foo_policy.name())
            .await
            .unwrap();
        entity_repo
            .attach_policy(entity.name(), bar_policy.name())
            .await
            .unwrap();

        // Now create token for "John"
        let token = TokenEntry::new(entity.name().to_string(), Duration::hours(1));
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
