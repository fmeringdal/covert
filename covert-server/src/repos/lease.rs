use std::{cmp::min, sync::Arc};

use chrono::{DateTime, Utc};
use covert_storage::EncryptedPool;

use crate::{
    error::{Error, ErrorType},
    LeaseEntry,
};

pub struct LeaseRepo {
    pool: Arc<EncryptedPool>,
}

impl Clone for LeaseRepo {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
        }
    }
}

impl LeaseRepo {
    pub fn new(pool: Arc<EncryptedPool>) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip_all, fields(lease_id = le.id))]
    pub async fn create(&self, le: &LeaseEntry) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO LEASES (id, issued_mount_path, revoke_path, revoke_data, renew_path, renew_data, issued_at, expires_at, last_renewal_time, failed_revocation_attempts)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(le.failed_revocation_attempts)
        .execute(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .and_then(|res| if res.rows_affected() == 1 {
            Ok(())
        } else {
            Err(ErrorType::InternalError(anyhow::Error::msg("failed to insert lease")).into())
        })
    }

    #[tracing::instrument(skip(self))]
    pub async fn pull(&self, count: u32, before: DateTime<Utc>) -> Result<Vec<LeaseEntry>, Error> {
        let count = min(count, 100);

        sqlx::query_as(
            "SELECT * FROM LEASES
                WHERE expires_at <= $1 
                ORDER BY expires_at
                LIMIT $2",
        )
        .bind(before)
        .bind(count)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn peek(&self) -> Result<Option<LeaseEntry>, Error> {
        sqlx::query_as(
            "SELECT * FROM LEASES
                ORDER BY expires_at ASC
                LIMIT 1",
        )
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(Into::into)
    }

    #[tracing::instrument(skip_all, fields(lease_id))]
    pub async fn increment_failed_revocation_attempts(
        &self,
        lease_id: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), Error> {
        sqlx::query(
            "UPDATE LEASES 
            SET failed_revocation_attempts = failed_revocation_attempts + 1,
                expires_at = $1
            WHERE id = $2",
        )
        .bind(expires_at)
        .bind(lease_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(Into::into)
        .and_then(|res| {
            if res.rows_affected() == 1 {
                Ok(())
            } else {
                Err(ErrorType::NotFound(format!(
                    "failed to increment failed revocation attempt for lease: `{lease_id}`"
                ))
                .into())
            }
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
    pub async fn list_by_mount_prefix(&self, path_prefix: &str) -> Result<Vec<LeaseEntry>, Error> {
        let prefix_pattern = format!("{path_prefix}%");
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
    use chrono::{Duration, Utc};
    use covert_types::{
        backend::BackendType,
        mount::{MountConfig, MountEntry},
    };
    use uuid::Uuid;

    use crate::repos::mount::{tests::pool, MountRepo};

    use super::*;

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn crud() {
        let pool = Arc::new(pool().await);
        let mount_repo = MountRepo::new(Arc::clone(&pool));
        let lease_repo = LeaseRepo::new(Arc::clone(&pool));

        // Create postgres mount
        let userpass_mount = MountEntry {
            id: Uuid::new_v4(),
            backend_type: BackendType::Postgres,
            config: MountConfig::default(),
            path: "psql/".into(),
        };
        mount_repo.create(&userpass_mount).await.unwrap();

        let expires_at = Utc::now();

        // Nothing in beginning
        assert!(lease_repo.peek().await.unwrap().is_none());

        // Create some leases
        let mut lease_foo_bar = LeaseEntry {
            id: "psql/foo/bar".into(),
            revoke_data: "data".into(),
            revoke_path: Some("psql/revoke-entry".into()),
            renew_data: "data".into(),
            renew_path: Some("psql/renew-entry".into()),
            expires_at,
            issued_at: Utc::now(),
            issued_mount_path: userpass_mount.path.clone(),
            last_renewal_time: Utc::now(),
            failed_revocation_attempts: 0,
        };
        assert!(lease_repo.create(&lease_foo_bar).await.is_ok());
        assert_eq!(
            lease_repo.peek().await.unwrap(),
            Some(lease_foo_bar.clone())
        );

        let mut lease_bar_foo = LeaseEntry {
            id: "psql/bar/foo".into(),
            revoke_data: "data".into(),
            revoke_path: Some("psql/revoke-entry".into()),
            renew_data: "data".into(),
            renew_path: Some("psql/renew-entry".into()),
            expires_at: lease_foo_bar.expires_at - Duration::milliseconds(1),
            issued_at: Utc::now(),
            issued_mount_path: userpass_mount.path.clone(),
            last_renewal_time: Utc::now(),
            failed_revocation_attempts: 0,
        };
        assert!(lease_repo.create(&lease_bar_foo).await.is_ok());
        assert_eq!(
            lease_repo.peek().await.unwrap(),
            Some(lease_bar_foo.clone())
        );

        // Pull
        assert_eq!(
            lease_repo.pull(100, expires_at).await.unwrap(),
            vec![lease_bar_foo.clone(), lease_foo_bar.clone()]
        );
        assert_eq!(lease_repo.pull(1, expires_at).await.unwrap().len(), 1);
        assert!(lease_repo
            .pull(100, expires_at - Duration::seconds(1))
            .await
            .unwrap()
            .is_empty(),);

        // List all
        assert_eq!(
            lease_repo.list().await.unwrap(),
            vec![lease_foo_bar.clone(), lease_bar_foo.clone()]
        );

        // List by mount path prefix
        assert_eq!(
            lease_repo
                .list_by_mount_prefix(&userpass_mount.path)
                .await
                .unwrap(),
            vec![lease_foo_bar.clone(), lease_bar_foo.clone()]
        );
        assert_eq!(
            lease_repo
                .list_by_mount_prefix("random_foo_bar/")
                .await
                .unwrap(),
            vec![]
        );

        // Renew
        lease_foo_bar.expires_at += chrono::Duration::hours(1);
        lease_foo_bar.last_renewal_time += chrono::Duration::hours(1);
        assert!(lease_repo
            .renew(
                &lease_foo_bar.id,
                lease_bar_foo.expires_at,
                lease_foo_bar.last_renewal_time
            )
            .await
            .is_ok());

        // Lookup by id
        assert_eq!(
            lease_repo.lookup(lease_foo_bar.id()).await.unwrap(),
            Some(lease_foo_bar.clone())
        );

        // Delete one lease
        assert!(lease_repo.delete(lease_foo_bar.id()).await.unwrap());

        // And it should be gone
        assert_eq!(
            lease_repo.list().await.unwrap(),
            vec![lease_bar_foo.clone()]
        );

        // Increment on failure
        lease_bar_foo.expires_at += Duration::seconds(10);
        lease_bar_foo.failed_revocation_attempts += 1;
        assert!(lease_repo
            .increment_failed_revocation_attempts(lease_bar_foo.id(), lease_bar_foo.expires_at)
            .await
            .is_ok());

        let lease_bar_foo_from_store = lease_repo
            .lookup(lease_bar_foo.id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            lease_bar_foo.expires_at,
            lease_bar_foo_from_store.expires_at
        );
        assert_eq!(
            lease_bar_foo.failed_revocation_attempts,
            lease_bar_foo_from_store.failed_revocation_attempts
        );
    }
}
