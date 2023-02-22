use std::{str::FromStr, sync::Arc, time::Duration};

use covert_storage::EncryptedPool;
use covert_types::{
    backend::BackendType,
    mount::{MountConfig, MountEntry},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, ErrorType};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct MountEntryRaw {
    pub id: String,
    pub path: String,
    pub default_lease_ttl: i64,
    pub max_lease_ttl: i64,
    pub variant: String,
    pub namespace_id: String,
}

impl TryFrom<MountEntryRaw> for MountEntry {
    type Error = Error;

    fn try_from(value: MountEntryRaw) -> Result<MountEntry, Error> {
        let id = Uuid::from_str(&value.id).map_err(|_| {
            ErrorType::BadData(format!("`{}` is not a valid uuid mount id", value.id))
        })?;
        let backend_type = BackendType::from_str(&value.variant).map_err(|_| {
            ErrorType::BadData(format!("`{}` is not a valid backend type", value.variant))
        })?;
        let default_lease_ttl = u64::try_from(value.default_lease_ttl).unwrap_or(u64::MAX);
        let max_lease_ttl = u64::try_from(value.max_lease_ttl).unwrap_or(u64::MAX);

        Ok(MountEntry {
            id,
            path: value.path,
            config: MountConfig {
                default_lease_ttl: Duration::from_millis(default_lease_ttl),
                max_lease_ttl: Duration::from_millis(max_lease_ttl),
            },
            backend_type,
            namespace_id: value.namespace_id,
        })
    }
}

pub struct MountRepo {
    pool: Arc<EncryptedPool>,
}

impl Clone for MountRepo {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
        }
    }
}

impl MountRepo {
    pub fn new(pool: Arc<EncryptedPool>) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip(self))]
    pub async fn create(&self, mount: &MountEntry) -> Result<(), Error> {
        let max_lease_ttl =
            i64::try_from(mount.config.max_lease_ttl.as_millis()).unwrap_or(i64::MAX);
        let default_lease_ttl =
            i64::try_from(mount.config.default_lease_ttl.as_millis()).unwrap_or(i64::MAX);
        sqlx::query(
            "INSERT INTO MOUNTS (id, path, variant, max_lease_ttl, default_lease_ttl, namespace_id)
            VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(mount.id.to_string())
        .bind(&mount.path)
        .bind(mount.backend_type.to_string())
        .bind(max_lease_ttl)
        .bind(default_lease_ttl)
        .bind(&mount.namespace_id)
        .execute(self.pool.as_ref())
        .await
        .map(|_| ())
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn set_config(
        &self,
        path: &str,
        namespace_id: &str,
        config: &MountConfig,
    ) -> Result<(), Error> {
        let max_lease_ttl = i64::try_from(config.max_lease_ttl.as_millis()).unwrap_or(i64::MAX);
        let default_lease_ttl =
            i64::try_from(config.default_lease_ttl.as_millis()).unwrap_or(i64::MAX);

        sqlx::query(
            "UPDATE MOUNTS SET 
                    max_lease_ttl = ?,
                    default_lease_ttl = ?
                WHERE path = ? AND namespace_id = ?",
        )
        .bind(max_lease_ttl)
        .bind(default_lease_ttl)
        .bind(path)
        .bind(namespace_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .and_then(|res| {
            if res.rows_affected() == 1 {
                Ok(())
            } else {
                Err(ErrorType::NotFound(format!("Mount at `{path}` not found")).into())
            }
        })
    }

    #[tracing::instrument(skip_all)]
    pub async fn list(&self, namespace_id: &str) -> Result<Vec<MountEntry>, Error> {
        sqlx::query_as("SELECT * FROM MOUNTS WHERE namespace_id = ? ORDER BY path ASC")
            .bind(namespace_id)
            .fetch_all(self.pool.as_ref())
            .await
            .map(|mounts: Vec<MountEntryRaw>| {
                mounts
                    .into_iter()
                    .filter_map(|m| m.try_into().ok())
                    .collect()
            })
            .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_by_path(
        &self,
        path: &str,
        namespace_id: &str,
    ) -> Result<Option<MountEntry>, Error> {
        sqlx::query_as("SELECT * FROM MOUNTS WHERE path = ? AND namespace_id = ?")
            .bind(path)
            .bind(namespace_id)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .and_then(|m: Option<MountEntryRaw>| m.map(TryInto::try_into).transpose())
    }

    #[tracing::instrument(skip(self))]
    pub async fn longest_prefix(
        &self,
        path: &str,
        namespace_id: &str,
    ) -> Result<Option<MountEntry>, Error> {
        sqlx::query_as(
            "SELECT * FROM MOUNTS 
            WHERE namespace_id = ? AND ? LIKE (path || '%')
            ORDER BY length(path) DESC LIMIT 1",
        )
        .bind(namespace_id)
        .bind(path)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .and_then(|m: Option<MountEntryRaw>| m.map(TryInto::try_into).transpose())
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_by_path(&self, path: &str, namespace_id: &str) -> Result<bool, Error> {
        sqlx::query("DELETE FROM MOUNTS WHERE path = ? AND namespace_id = ?")
            .bind(path)
            .bind(namespace_id)
            .execute(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .map(|res| res.rows_affected() == 1)
    }
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;

    use crate::repos::namespace::{Namespace, NamespaceRepo};

    use super::*;

    pub async fn pool() -> EncryptedPool {
        let pool = EncryptedPool::new_tmp();

        crate::migrations::migrate_ecrypted_db(&pool).await.unwrap();

        pool
    }

    #[tokio::test]
    async fn crud() {
        let pool = Arc::new(pool().await);
        let store = MountRepo::new(Arc::clone(&pool));
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ns_repo.create(&ns).await.unwrap();

        let mut me = MountEntry {
            id: Uuid::new_v4(),
            backend_type: BackendType::Kv,
            config: MountConfig {
                default_lease_ttl: Duration::from_secs(30),
                max_lease_ttl: Duration::from_secs(60),
            },
            path: "foo".into(),
            namespace_id: ns.id.clone(),
        };
        assert!(store.create(&me).await.is_ok());
        assert_eq!(store.list(&ns.id).await.unwrap(), vec![me.clone()]);

        let new_config = MountConfig {
            default_lease_ttl: Duration::ZERO,
            max_lease_ttl: Duration::ZERO,
        };
        me.config = new_config.clone();

        assert!(store
            .set_config(&me.path, &me.namespace_id, &new_config)
            .await
            .is_ok());
        assert_eq!(store.list(&ns.id).await.unwrap(), vec![me.clone()]);

        assert_eq!(
            store.get_by_path(&me.path, &ns.id).await.unwrap(),
            Some(me.clone())
        );
        assert_eq!(
            store
                .get_by_path(&format!("{}foo", me.path), &ns.id)
                .await
                .unwrap(),
            None
        );

        assert!(store.remove_by_path(&me.path, &ns.id).await.unwrap());
        assert_eq!(store.get_by_path(&me.path, &ns.id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn longest_prefix() {
        let pool = Arc::new(pool().await);
        let store = MountRepo::new(Arc::clone(&pool));
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ns_repo.create(&ns).await.unwrap();

        assert!(store.longest_prefix("/", &ns.id).await.unwrap().is_none());

        let mut ids = HashMap::new();

        for path in ["/foo", "/foo/bar", "/foo/bar/baz"] {
            let me = MountEntry {
                id: Uuid::new_v4(),
                backend_type: BackendType::Kv,
                config: MountConfig {
                    default_lease_ttl: Duration::from_secs(30),
                    max_lease_ttl: Duration::from_secs(60),
                },
                path: path.into(),
                namespace_id: ns.id.clone(),
            };
            assert!(store.create(&me).await.is_ok());
            ids.insert(path, me.id);
        }

        assert!(store.longest_prefix("/", &ns.id).await.unwrap().is_none());

        let tests = [
            ("/foo", "/foo"),
            ("/foo/ba", "/foo"),
            ("/foo/bar", "/foo/bar"),
            ("/foo/bar/ba", "/foo/bar"),
            ("/foo/bar/baz", "/foo/bar/baz"),
            ("/foo/bar/baz/", "/foo/bar/baz"),
        ];
        for (p1, p2) in tests {
            assert_eq!(
                store
                    .longest_prefix(p1, &ns.id)
                    .await
                    .unwrap()
                    .map(|me| me.id),
                Some(ids.get(p2).copied().unwrap())
            );
        }
    }
}
