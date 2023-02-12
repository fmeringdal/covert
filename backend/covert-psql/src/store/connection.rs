use covert_storage::BackendStoragePool;
use covert_types::psql::ConnectionConfig;

use crate::error::Error;

pub const CONNECTION_TABLE: &str = "CONNECTION";

pub struct ConnectionStore {
    pool: BackendStoragePool,
}

impl ConnectionStore {
    pub fn new(pool: BackendStoragePool) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip_all)]
    pub async fn set(&self, conn: &ConnectionConfig) -> Result<(), Error> {
        self.pool
            .query(
                &format!("INSERT OR REPLACE INTO {CONNECTION_TABLE} (lock, connection_url, max_open_connections) 
                    VALUES (?, ?, ?)"),
            )?
            .bind(1)
            .bind(&conn.connection_url)
            .bind(conn.max_open_connections)
            .execute()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn get(&self) -> Result<Option<ConnectionConfig>, Error> {
        self.pool
            .query(&format!("SELECT * FROM {CONNECTION_TABLE}"))?
            .fetch_optional()
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove(&self) -> Result<bool, Error> {
        self.pool
            .query(&format!("DELETE FROM {CONNECTION_TABLE}"))?
            .execute()
            .await
            .map(|res| res.rows_affected() == 1)
            .map_err(Into::into)
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use covert_storage::{migrator::migrate_backend, BackendStoragePool, EncryptedPool};
    use covert_types::psql::ConnectionConfig;

    use crate::{store::connection::ConnectionStore, Migrations};

    pub async fn setup_context() -> BackendStoragePool {
        let pool = Arc::new(EncryptedPool::new_tmp());

        let storage = BackendStoragePool::new(
            "foo".into(),
            "629d847db7c9492ba69aed520cd1997d".into(),
            pool,
        );

        migrate_backend::<Migrations>(&storage).await.unwrap();

        storage
    }

    #[sqlx::test]
    async fn crud() {
        let pool = setup_context().await;
        let store = ConnectionStore::new(pool);

        assert!(store.get().await.unwrap().is_none());

        let mut conf = ConnectionConfig {
            connection_url: "https://example.com".into(),
            max_open_connections: 10,
        };
        assert!(store.set(&conf).await.is_ok());
        assert_eq!(store.get().await.unwrap(), Some(conf.clone()));

        conf.max_open_connections += 1;
        assert!(store.set(&conf).await.is_ok());
        assert_eq!(store.get().await.unwrap(), Some(conf.clone()));

        assert!(store.remove().await.unwrap());
        assert!(store.get().await.unwrap().is_none());
    }
}
