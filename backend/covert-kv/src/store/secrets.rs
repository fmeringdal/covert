use covert_storage::BackendStoragePool;

use crate::{domain::secret::Secret, error::Error};

const SECRETS_TABLE: &str = "SECRETS";

#[derive(Debug, sqlx::FromRow)]
pub struct SecretMetadata {
    pub max_version: u32,
    pub min_version: u32,
}

#[derive(Debug)]
pub struct Repo {
    pool: BackendStoragePool,
}

impl Repo {
    pub fn new(pool: BackendStoragePool) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip_all)]
    pub async fn get(&self, key: &str, version: u32) -> Result<Option<Secret>, Error> {
        self.pool
            .query(&format!(
                "SELECT * FROM {SECRETS_TABLE} WHERE
                    key = $1 AND
                    version = $2"
            ))?
            .bind(key)
            .bind(version)
            .fetch_optional()
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn version_metadata(&self, key: &str) -> Result<Option<SecretMetadata>, Error> {
        self.pool
            .query(&format!(
                "SELECT MIN(version) AS min_version, MAX(version) AS max_version FROM {SECRETS_TABLE} WHERE
                key = $1"
            ))?
            .bind(key)
            .fetch_optional::<SecretMetadata>()
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn soft_delete(&self, key: &str, versions: &[u32]) -> Result<Vec<u32>, Error> {
        // sqlx sqlite doesn't support Vec<T>: Encode
        let mut not_deleted = vec![];
        for version in versions {
            match self
                .pool
                .query(&format!(
                    "UPDATE {SECRETS_TABLE} 
                 SET
                     deleted = TRUE
                 WHERE
                     key = $1 AND
                     version = $2
             "
                ))?
                .bind(key)
                .bind(version)
                .execute()
                .await
                .map(|res| res.rows_affected() == 1)
            {
                Ok(true) => (),
                _ => not_deleted.push(*version),
            }
        }

        Ok(not_deleted)
    }

    #[tracing::instrument(skip_all)]
    pub async fn recover(&self, key: &str, versions: &[u32]) -> Result<Vec<u32>, Error> {
        // sqlx sqlite doesn't support Vec<T>: Encode
        let mut not_recovered = vec![];
        for version in versions {
            match self
                .pool
                .query(&format!(
                    "UPDATE {SECRETS_TABLE} 
                 SET
                    deleted = FALSE
                 WHERE
                    key = $1 AND
                    version = $2 AND
                    destroyed = FALSE"
                ))?
                .bind(key)
                .bind(version)
                .execute()
                .await
                .map(|res| res.rows_affected() == 1)
            {
                Ok(true) => (),
                _ => not_recovered.push(*version),
            }
        }

        Ok(not_recovered)
    }

    #[tracing::instrument(skip_all)]
    pub async fn hard_delete(&self, key: &str, versions: &[u32]) -> Result<Vec<u32>, Error> {
        // sqlx sqlite doesn't support Vec<T>: Encode
        let mut not_deleted = vec![];
        for version in versions {
            match self
                .pool
                .query(&format!(
                    "UPDATE {SECRETS_TABLE} 
                SET
                    destroyed = TRUE,
                    deleted = TRUE,
                    value = NULL
                WHERE
                    key = $1 AND
                    version = $2
                "
                ))?
                .bind(key)
                .bind(version)
                .execute()
                .await
                .map(|res| res.rows_affected() == 1)
            {
                Ok(true) => (),
                _ => not_deleted.push(*version),
            }
        }

        Ok(not_deleted)
    }

    #[tracing::instrument(skip_all)]
    pub async fn insert(&self, secret: &Secret) -> Result<bool, Error> {
        self
            .pool
            .query(&format!(
                "INSERT INTO {SECRETS_TABLE} (key, version, value, created_time, deleted, destroyed) 
                    VALUES ($1, $2, $3, $4, $5, $6)"
            ))?
            .bind(&secret.key)
            .bind(secret.version)
            .bind(&secret.value)
            .bind(secret.created_time)
            .bind(secret.deleted)
            .bind(secret.destroyed)
            .execute()
            .await
            .map(|res| res.rows_affected() == 1)
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn prune_old_versions(&self, key: &str, max_versions: u32) -> Result<(), Error> {
        self.pool
            .query(&format!(
                "DELETE FROM {SECRETS_TABLE} WHERE
                    key = $1 AND
                    version <= (SELECT MAX(version) FROM {SECRETS_TABLE} WHERE key = $1) - $2"
            ))?
            .bind(key)
            .bind(max_versions)
            .execute()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

#[cfg(test)]
pub mod tests {
    use std::{collections::HashMap, sync::Arc};

    use chrono::Utc;
    use covert_storage::{migrator::migrate_backend, BackendStoragePool, EncryptedPool};

    use crate::{context::Context, domain::secret::Secret, Migrations};

    pub async fn setup() -> Context {
        let pool = Arc::new(EncryptedPool::new_tmp());

        let storage = BackendStoragePool::new("foo_", pool);

        migrate_backend::<Migrations>(&storage).await.unwrap();

        Context::new(storage)
    }

    #[sqlx::test]
    fn insert() {
        let ctx = setup().await;
        let repo = &ctx.repos.secrets;

        let secret = Secret {
            key: "foo".into(),
            value: Some("bar".into()),
            created_time: Utc::now(),
            deleted: false,
            destroyed: false,
            version: 1,
        };
        let success = repo.insert(&secret).await.unwrap();
        assert!(success);

        // Query
        let res = repo.get(&secret.key, secret.version).await.unwrap();
        assert_eq!(res, Some(secret));

        // Insert with same version should fail
        let secret = Secret {
            key: "foo".into(),
            value: Some("bars".into()),
            created_time: Utc::now(),
            deleted: false,
            destroyed: false,
            version: 1,
        };
        let res = repo.insert(&secret).await;
        assert!(res.is_err());

        // Insert with same version but different key is fine
        let secret = Secret {
            key: "notfoo".into(),
            value: Some("bars".into()),
            created_time: Utc::now(),
            deleted: false,
            destroyed: false,
            version: 1,
        };
        let success = repo.insert(&secret).await.unwrap();
        assert!(success);
    }

    #[sqlx::test]
    fn prune_old_versions() {
        let ctx = setup().await;
        let repo = &ctx.repos.secrets;

        let max_versions = 10;
        let max_version = 31;
        let oldest_version = max_version - max_versions + 1;
        let versions = 1..=max_version;
        let key = "foo";

        let mut secrets = HashMap::new();
        for version in versions.clone() {
            let secret = Secret {
                key: key.into(),
                value: Some("bar".into()),
                created_time: Utc::now(),
                deleted: false,
                destroyed: false,
                version,
            };
            let success = repo.insert(&secret).await.unwrap();
            secrets.insert(version, secret);
            assert!(success);
        }

        assert_eq!(
            repo.version_metadata(key)
                .await
                .unwrap()
                .map(|res| res.max_version),
            Some(max_version)
        );
        assert!(repo.prune_old_versions(key, max_versions).await.is_ok());
        assert_eq!(
            repo.version_metadata(key)
                .await
                .unwrap()
                .map(|res| res.max_version),
            Some(max_version)
        );

        for version in versions {
            let res = repo.get(key, version).await.unwrap();
            if version >= oldest_version {
                assert_eq!(res.as_ref(), Some(&secrets[&version]));
            } else {
                assert!(res.is_none());
            }
        }
    }

    #[sqlx::test]
    fn hard_delete() {
        let ctx = setup().await;
        let repo = &ctx.repos.secrets;

        let key = "foo";
        let version = 1;
        let secret = Secret {
            key: key.into(),
            value: Some("bar".into()),
            created_time: Utc::now(),
            deleted: false,
            destroyed: false,
            version,
        };
        let success = repo.insert(&secret).await.unwrap();
        assert!(success);

        // Query
        let res = repo.get(&secret.key, version).await.unwrap();
        assert_eq!(res.as_ref(), Some(&secret));

        // Hard delete
        assert!(repo.hard_delete(key, &[version]).await.unwrap().is_empty());

        // Check that value is empty
        let res = repo.get(&secret.key, version).await.unwrap().unwrap();
        assert!(res.value.is_none());
        assert!(res.destroyed);
    }

    #[sqlx::test]
    fn soft_delete_and_recover() {
        let ctx = setup().await;
        let repo = &ctx.repos.secrets;

        let key = "foo";
        let version = 1;
        let secret = Secret {
            key: key.into(),
            value: Some("bar".into()),
            created_time: Utc::now(),
            deleted: false,
            destroyed: false,
            version,
        };
        let success = repo.insert(&secret).await.unwrap();
        assert!(success);

        // Query
        let res = repo.get(&secret.key, version).await.unwrap();
        assert_eq!(res.as_ref(), Some(&secret));

        // Soft delete
        assert!(repo.soft_delete(key, &[version]).await.unwrap().is_empty());

        // Check that value is *NOT* empty
        let res = repo.get(&secret.key, version).await.unwrap().unwrap();
        assert!(res.value.is_some());
        assert!(res.deleted);
        assert!(!res.destroyed);

        // Now recover
        assert!(repo.recover(key, &[version]).await.unwrap().is_empty());

        // Query again
        let res = repo.get(&secret.key, version).await.unwrap().unwrap();
        assert!(res.value.is_some());
        assert!(!res.deleted);
        assert!(!res.destroyed);
    }
}
