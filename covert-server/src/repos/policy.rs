use std::sync::Arc;

use covert_storage::EncryptedPool;
use covert_types::policy::Policy;

use crate::error::{Error, ErrorType};

pub struct PolicyRepo {
    pool: Arc<EncryptedPool>,
}

impl Clone for PolicyRepo {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
        }
    }
}

impl PolicyRepo {
    pub fn new(pool: Arc<EncryptedPool>) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip(self))]
    pub async fn lookup(&self, name: &str, namespace_id: &str) -> Result<Option<Policy>, Error> {
        sqlx::query_as("SELECT * FROM POLICIES WHERE name = ? AND namespace_id = ?")
            .bind(name)
            .bind(namespace_id)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .and_then(|p: Option<PolicyRaw>| p.map(TryInto::try_into).transpose())
    }

    #[tracing::instrument(skip(self))]
    pub async fn batch_lookup(&self, policy_names: &[String], namespace_id: &str) -> Vec<Policy> {
        let mut futures = Vec::with_capacity(policy_names.len());
        for name in policy_names {
            futures.push(self.lookup(name, namespace_id));
        }
        futures::future::join_all(futures)
            .await
            .into_iter()
            .filter_map(|res| match res {
                Ok(policy) => policy,
                Err(_) => None,
            })
            .collect()
    }

    #[tracing::instrument(skip(self))]
    pub async fn list(&self, namespace_id: &str) -> Result<Vec<Policy>, Error> {
        sqlx::query_as("SELECT * FROM POLICIES WHERE namespace_id = ?")
            .bind(namespace_id)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .map(|policies: Vec<PolicyRaw>| {
                policies
                    .into_iter()
                    .filter_map(|p| p.try_into().ok())
                    .collect()
            })
    }

    #[tracing::instrument(skip(self))]
    pub async fn create(&self, policy: &Policy) -> Result<(), Error> {
        let Policy {
            name,
            paths,
            namespace_id,
        } = policy;

        let policies = serde_json::to_string(paths)
            .map_err(|_| ErrorType::BadRequest("Invalid policy format".to_string()))?;

        sqlx::query("INSERT INTO POLICIES (name, policy, namespace_id) VALUES (?, ?, ?)")
            .bind(name)
            .bind(policies)
            .bind(namespace_id)
            .execute(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .map(|_| ())
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove(&self, name: &str, namespace_id: &str) -> Result<bool, Error> {
        sqlx::query("DELETE FROM POLICIES WHERE name = ? AND namespace_id = ?")
            .bind(name)
            .bind(namespace_id)
            .execute(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .map(|res| res.rows_affected() == 1)
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct PolicyRaw {
    name: String,
    policy: String,
    namespace_id: String,
}

impl TryFrom<PolicyRaw> for Policy {
    type Error = Error;

    fn try_from(p: PolicyRaw) -> Result<Self, Self::Error> {
        serde_json::from_str(&p.policy)
            .map_err(|_| {
                ErrorType::BadData(format!("Unable to parse policy `{}`", p.policy)).into()
            })
            .map(|paths| Policy {
                name: p.name,
                paths,
                namespace_id: p.namespace_id,
            })
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::repos::{
        mount::tests::pool,
        namespace::{Namespace, NamespaceRepo},
    };

    use super::*;

    #[tokio::test]
    async fn crud() {
        let pool = Arc::new(pool().await);
        let store = PolicyRepo::new(Arc::clone(&pool));
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ns_repo.create(&ns).await.unwrap();

        let policy = Policy {
            name: "foo".into(),
            paths: vec![],
            namespace_id: ns.id.clone(),
        };
        assert!(store.create(&policy).await.is_ok());
        assert_eq!(store.list(&ns.id).await.unwrap(), vec![policy.clone()]);
        assert_eq!(
            store.lookup(&policy.name, &ns.id).await.unwrap(),
            Some(policy.clone())
        );
        assert_eq!(
            store.batch_lookup(&[policy.name.clone()], &ns.id).await,
            vec![policy.clone()]
        );

        assert!(store.remove(&policy.name, &ns.id).await.unwrap());
        assert!(store.list(&ns.id).await.unwrap().is_empty());
    }
}
