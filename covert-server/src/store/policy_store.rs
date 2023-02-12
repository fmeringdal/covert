use std::sync::Arc;

use covert_storage::EncryptedPool;
use covert_types::policy::Policy;

use crate::error::{Error, ErrorType};

pub struct PolicyStore {
    pool: Arc<EncryptedPool>,
}

impl PolicyStore {
    pub fn new(pool: Arc<EncryptedPool>) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip(self))]
    pub async fn lookup(&self, name: &str) -> Result<Option<Policy>, Error> {
        sqlx::query_as("SELECT name, policy FROM POLICIES WHERE name = ?")
            .bind(name)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .and_then(|p: Option<PolicyRaw>| p.map(TryInto::try_into).transpose())
    }

    #[tracing::instrument(skip(self))]
    pub async fn batch_lookup(&self, policy_names: &[String]) -> Vec<Policy> {
        let mut futures = Vec::with_capacity(policy_names.len());
        for name in policy_names {
            futures.push(self.lookup(name));
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
    pub async fn list(&self) -> Result<Vec<Policy>, Error> {
        sqlx::query_as("SELECT name, policy FROM POLICIES")
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
        let Policy { name, paths } = policy;

        let policies = serde_json::to_string(paths)
            .map_err(|_| ErrorType::BadRequest("Invalid policy format".to_string()))?;

        sqlx::query("INSERT INTO POLICIES (name, policy) VALUES (?, ?)")
            .bind(name)
            .bind(policies)
            .execute(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .map(|_| ())
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove(&self, name: &str) -> Result<bool, Error> {
        sqlx::query("DELETE FROM POLICIES WHERE name = ?")
            .bind(name)
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
            })
    }
}

#[cfg(test)]
mod tests {
    use crate::store::mount_store::tests::pool;

    use super::*;

    #[tokio::test]
    async fn crud() {
        let pool = pool().await.pool;
        let store = PolicyStore::new(Arc::new(pool));

        let policy = Policy {
            name: "foo".into(),
            paths: vec![],
        };
        assert!(store.create(&policy).await.is_ok());
        assert_eq!(store.list().await.unwrap(), vec![policy.clone()]);
        assert_eq!(
            store.lookup(&policy.name).await.unwrap(),
            Some(policy.clone())
        );
        assert_eq!(
            store.batch_lookup(&[policy.name.clone()]).await,
            vec![policy.clone()]
        );

        assert!(store.remove(&policy.name).await.unwrap());
        assert!(store.list().await.unwrap().is_empty());
    }
}
