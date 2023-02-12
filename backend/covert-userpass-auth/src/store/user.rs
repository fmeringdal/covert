use covert_storage::BackendStoragePool;

use crate::{error::Error, User};

const USERS_TABLE: &str = "USERS";

#[derive(Debug)]
pub struct UsersRepo {
    pool: BackendStoragePool,
}

impl UsersRepo {
    pub fn new(pool: BackendStoragePool) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip_all)]
    pub async fn create(&self, user: &User) -> Result<bool, Error> {
        self.pool
            .query(&format!(
                "INSERT INTO {USERS_TABLE} (username, password) 
                    VALUES ($1, $2)"
            ))?
            .bind(&user.username)
            .bind(&user.password)
            .execute()
            .await
            .map(|res| res.rows_affected() == 1)
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove(&self, username: &str) -> Result<bool, Error> {
        self.pool
            .query(&format!("DELETE FROM {USERS_TABLE} WHERE username = ?"))?
            .bind(username)
            .execute()
            .await
            .map(|res| res.rows_affected() == 1)
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn list(&self) -> Result<Vec<User>, Error> {
        self.pool
            .query(&format!("SELECT * FROM {USERS_TABLE}"))?
            .fetch_all()
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn get(&self, username: &str) -> Result<Option<User>, Error> {
        self.pool
            .query(&format!("SELECT * FROM {USERS_TABLE} WHERE username = ?"))?
            .bind(username)
            .fetch_optional()
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn update_password(&self, username: &str, password: &str) -> Result<(), Error> {
        self.pool
            .query(&format!(
                "UPDATE {USERS_TABLE} SET
                password = ?
                WHERE username = ?"
            ))?
            .bind(password)
            .bind(username)
            .execute()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use covert_storage::{migrator::migrate_backend, EncryptedPool};
    use tempfile::TempDir;

    use crate::Migrations;

    use super::*;

    pub async fn setup_context() -> (BackendStoragePool, TempDir) {
        let tmpdir = tempfile::tempdir().unwrap();
        let file_path = tmpdir
            .path()
            .join("db-storage")
            .to_str()
            .unwrap()
            .to_string();

        let pool = Arc::new(EncryptedPool::new(&file_path));
        let master_key = pool.initialize().unwrap().unwrap();
        pool.unseal(master_key.clone()).unwrap();

        let storage = BackendStoragePool::new(
            "foo".into(),
            "629d847db7c9492ba69aed520cd1997d".into(),
            pool,
        );

        migrate_backend::<Migrations>(&storage).await.unwrap();

        (storage, tmpdir)
    }

    #[sqlx::test]
    async fn crud() {
        let pool = setup_context().await.0;
        let store = UsersRepo::new(pool);

        let user = User {
            username: "foo".into(),
            password: "pass".into(),
        };
        assert!(store.create(&user).await.is_ok());

        assert_eq!(store.list().await.unwrap(), vec![user.clone()]);
        assert_eq!(store.get(&user.username).await.unwrap(), Some(user.clone()));
        assert_eq!(store.get("not existing username").await.unwrap(), None);

        let newpass = "newpass";
        assert!(store.update_password(&user.username, newpass).await.is_ok());
        assert_eq!(
            store.get(&user.username).await.unwrap(),
            Some(User {
                username: user.username.clone(),
                password: newpass.to_string()
            })
        );

        assert!(store.remove(&user.username).await.is_ok());
        assert!(store.get(&user.username).await.unwrap().is_none());
    }
}
