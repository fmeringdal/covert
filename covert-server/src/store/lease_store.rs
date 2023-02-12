use std::sync::Arc;

use chrono::{DateTime, Utc};
use covert_storage::EncryptedPool;

use crate::{
    error::{Error, ErrorType},
    LeaseEntry,
};

#[derive(Debug)]
pub struct LeaseStore {
    pool: Arc<EncryptedPool>,
}

impl LeaseStore {
    pub fn new(pool: Arc<EncryptedPool>) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip_all, fields(lease_id = le.id))]
    pub async fn create(&self, le: &LeaseEntry) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO LEASES (id, issued_mount_path, revoke_path, revoke_data, renew_path, renew_data, issued_at, expires_at, last_renewal_time)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&le.id)
        .bind(&le.issued_mount_path)
        .bind(&le.revoke_path)
        .bind(&le.revoke_data)
        .bind(&le.renew_path)
        .bind(&le.renew_data)
        .bind(le.issued_at)
        .bind(le.expires_at)
        .bind(le.last_renewal_time)
        .execute(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .and_then(|res| if res.rows_affected() == 1 {
            Ok(())
        } else {
            Err(ErrorType::InternalError(anyhow::Error::msg("failed to insert lease")).into())
        })
    }

    #[tracing::instrument(skip_all, fields(lease_id))]
    pub async fn renew(
        &self,
        lease_id: &str,
        expires_at: DateTime<Utc>,
        last_renewal_time: DateTime<Utc>,
    ) -> Result<(), Error> {
        sqlx::query(
            "UPDATE LEASES SET
                expires_at = ?,
                last_renewal_time = ?
                WHERE id = ?",
        )
        .bind(expires_at)
        .bind(last_renewal_time)
        .bind(lease_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .and_then(|res| {
            if res.rows_affected() == 1 {
                Ok(())
            } else {
                Err(ErrorType::NotFound(format!("Lease `{lease_id}` not found")).into())
            }
        })
    }

    #[tracing::instrument(skip_all, fields(lease_id))]
    pub async fn delete(&self, lease_id: &str) -> Result<bool, Error> {
        sqlx::query("DELETE FROM LEASES WHERE id = ?")
            .bind(lease_id)
            .execute(self.pool.as_ref())
            .await
            .map_err(Into::into)
            .map(|res| res.rows_affected() == 1)
    }

    #[tracing::instrument(skip_all)]
    pub async fn list(&self) -> Result<Vec<LeaseEntry>, Error> {
        sqlx::query_as("SELECT * FROM LEASES")
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn list_by_mount(&self, mount_path: &str) -> Result<Vec<LeaseEntry>, Error> {
        let prefix_pattern = format!("{mount_path}%");
        sqlx::query_as("SELECT * FROM LEASES WHERE issued_mount_path LIKE ?")
            .bind(prefix_pattern)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn lookup(&self, lease_id: &str) -> Result<Option<LeaseEntry>, Error> {
        sqlx::query_as("SELECT * FROM LEASES WHERE id LIKE ?")
            .bind(lease_id)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use covert_types::{backend::BackendType, mount::MountEntry};
    use uuid::Uuid;

    use crate::store::mount_store::{tests::pool, MountStore};

    use super::*;

    #[tokio::test]
    async fn crud() {
        let pool = Arc::new(pool().await);
        let mount_store = MountStore::new(Arc::clone(&pool));
        let lease_store = LeaseStore::new(Arc::clone(&pool));

        // Create postgres mount
        let userpass_mount = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: Default::default(),
            path: "psql/".into(),
        };
        mount_store.create(&userpass_mount).await.unwrap();

        // Create some leases
        let mut lease_foo_bar = LeaseEntry {
            id: "psql/foo/bar".into(),
            revoke_data: "data".into(),
            revoke_path: Some("psql/revoke-entry".into()),
            renew_data: "data".into(),
            renew_path: Some("psql/renew-entry".into()),
            expires_at: Utc::now(),
            issued_at: Utc::now(),
            issued_mount_path: userpass_mount.path.clone(),
            last_renewal_time: Utc::now(),
        };
        assert!(lease_store.create(&lease_foo_bar).await.is_ok());
        let lease_bar_foo = LeaseEntry {
            id: "psql/bar/foo".into(),
            revoke_data: "data".into(),
            revoke_path: Some("psql/revoke-entry".into()),
            renew_data: "data".into(),
            renew_path: Some("psql/renew-entry".into()),
            expires_at: Utc::now(),
            issued_at: Utc::now(),
            issued_mount_path: userpass_mount.path.clone(),
            last_renewal_time: Utc::now(),
        };
        assert!(lease_store.create(&lease_bar_foo).await.is_ok());

        // List all
        assert_eq!(
            lease_store.list().await.unwrap(),
            vec![lease_foo_bar.clone(), lease_bar_foo.clone()]
        );

        // List by mount path prefix
        assert_eq!(
            lease_store
                .list_by_mount(&userpass_mount.path)
                .await
                .unwrap(),
            vec![lease_foo_bar.clone(), lease_bar_foo.clone()]
        );
        assert_eq!(
            lease_store.list_by_mount("random_foo_bar/").await.unwrap(),
            vec![]
        );

        // Renew
        lease_foo_bar.expires_at += chrono::Duration::hours(1);
        lease_foo_bar.last_renewal_time += chrono::Duration::hours(1);
        assert!(lease_store
            .renew(
                &lease_foo_bar.id,
                lease_bar_foo.expires_at,
                lease_foo_bar.last_renewal_time
            )
            .await
            .is_ok());

        // Lookup by id
        assert_eq!(
            lease_store.lookup(lease_foo_bar.id()).await.unwrap(),
            Some(lease_foo_bar.clone())
        );

        // Delete one lease
        assert!(lease_store.delete(lease_foo_bar.id()).await.unwrap());

        // And it should be gone
        assert_eq!(
            lease_store.list().await.unwrap(),
            vec![lease_bar_foo.clone()]
        );
    }
}
