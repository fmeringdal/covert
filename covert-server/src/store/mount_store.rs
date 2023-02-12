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
}

impl TryFrom<MountEntryRaw> for MountEntry {
    type Error = Error;

    fn try_from(value: MountEntryRaw) -> Result<MountEntry, Error> {
        let uuid = Uuid::from_str(&value.id).map_err(|_| {
            ErrorType::BadData(format!("`{}` is not a valid uuid mount id", value.id))
        })?;
        let backend_type = BackendType::from_str(&value.variant).map_err(|_| {
            ErrorType::BadData(format!("`{}` is not a valid backend type", value.variant))
        })?;
        let default_lease_ttl = u64::try_from(value.default_lease_ttl).unwrap_or(u64::MAX);
        let max_lease_ttl = u64::try_from(value.max_lease_ttl).unwrap_or(u64::MAX);

        Ok(MountEntry {
            uuid,
            path: value.path,
            config: MountConfig {
                default_lease_ttl: Duration::from_millis(default_lease_ttl),
                max_lease_ttl: Duration::from_millis(max_lease_ttl),
            },
            backend_type,
        })
    }
}

pub struct MountStore {
    pool: Arc<EncryptedPool>,
}

impl MountStore {
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
            "INSERT INTO MOUNTS (id, path, variant, max_lease_ttl, default_lease_ttl)
            VALUES (?, ?, ?, ?, ?)",
        )
        .bind(mount.uuid.to_string())
        .bind(&mount.path)
        .bind(mount.backend_type.to_string())
        .bind(max_lease_ttl)
        .bind(default_lease_ttl)
        .execute(self.pool.as_ref())
        .await
        .map(|_| ())
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn set_config(&self, id: Uuid, config: &MountConfig) -> Result<(), Error> {
        let max_lease_ttl = i64::try_from(config.max_lease_ttl.as_millis()).unwrap_or(i64::MAX);
        let default_lease_ttl =
            i64::try_from(config.default_lease_ttl.as_millis()).unwrap_or(i64::MAX);

        sqlx::query(
            "UPDATE MOUNTS SET 
                    max_lease_ttl = ?,
                    default_lease_ttl = ?
                WHERE id = ?",
        )
        .bind(max_lease_ttl)
        .bind(default_lease_ttl)
        .bind(id.to_string())
        .execute(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .and_then(|res| {
            if res.rows_affected() == 1 {
                Ok(())
            } else {
                Err(ErrorType::NotFound(format!("Mount `{id}` not found")).into())
            }
        })
    }

    #[tracing::instrument(skip_all)]
    pub async fn list(&self) -> Result<Vec<MountEntry>, Error> {
        sqlx::query_as("SELECT * FROM MOUNTS")
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
    pub async fn get_by_path(&self, path: &str) -> Result<Option<MountEntry>, Error> {
        sqlx::query_as("SELECT * FROM MOUNTS WHERE path = ?")
            .bind(path)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .and_then(|m: Option<MountEntryRaw>| m.map(TryInto::try_into).transpose())
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_by_path(&self, path: &str) -> Result<bool, Error> {
        sqlx::query("DELETE FROM MOUNTS WHERE path = ?")
            .bind(path)
            .execute(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .map(|res| res.rows_affected() == 1)
    }
}

#[cfg(test)]
pub mod tests {
    use rust_embed::RustEmbed;
    use tempfile::TempDir;

    use crate::Core;

    use super::*;

    #[derive(RustEmbed)]
    #[folder = "migrations/"]
    struct Migrations;

    pub struct TestContext {
        pub pool: EncryptedPool,
        // Stored here just to prevent drop that would delete the storage file
        #[allow(dead_code)]
        pub dir: TempDir,
    }

    pub async fn pool() -> TestContext {
        let tmpdir = tempfile::tempdir().unwrap();
        let file_path = tmpdir
            .path()
            .join("covert-db-storage")
            .to_str()
            .unwrap()
            .to_string();

        let pool = EncryptedPool::new(&file_path);
        let master_key = pool.initialize().unwrap().unwrap();
        pool.unseal(master_key).unwrap();

        Core::migrate::<Migrations>(&pool).await.unwrap();

        TestContext { pool, dir: tmpdir }
    }

    #[tokio::test]
    async fn crud() {
        let pool = pool().await.pool;
        let store = MountStore::new(Arc::new(pool));

        let mut me = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Kv,
            config: MountConfig {
                default_lease_ttl: Duration::from_secs(30),
                max_lease_ttl: Duration::from_secs(60),
            },
            path: "foo".into(),
        };
        assert!(store.create(&me).await.is_ok());
        assert_eq!(store.list().await.unwrap(), vec![me.clone()]);

        let new_config = MountConfig {
            default_lease_ttl: Duration::ZERO,
            max_lease_ttl: Duration::ZERO,
        };
        me.config = new_config.clone();

        assert!(store.set_config(me.uuid, &new_config).await.is_ok());
        assert_eq!(store.list().await.unwrap(), vec![me.clone()]);

        assert_eq!(store.get_by_path(&me.path).await.unwrap(), Some(me.clone()));
        assert_eq!(
            store.get_by_path(&format!("{}foo", me.path)).await.unwrap(),
            None
        );

        assert!(store.remove_by_path(&me.path).await.unwrap());
        assert_eq!(store.get_by_path(&me.path).await.unwrap(), None);
    }
}
