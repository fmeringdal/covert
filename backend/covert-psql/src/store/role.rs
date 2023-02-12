use covert_storage::BackendStoragePool;

use crate::{error::Error, path_roles::RoleEntry};

pub const ROLES_TABLE: &str = "ROLES";

pub struct RoleStore {
    pool: BackendStoragePool,
}

impl RoleStore {
    pub fn new(pool: BackendStoragePool) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip_all)]
    pub async fn create(&self, name: &str, role: &RoleEntry) -> Result<bool, Error> {
        self.pool
            .query(&format!(
                "INSERT INTO {ROLES_TABLE} (name, sql, revocation_sql) 
                    VALUES (?, ?, ?)"
            ))?
            .bind(name)
            .bind(&role.sql)
            .bind(&role.revocation_sql)
            .execute()
            .await
            .map(|res| res.rows_affected() == 1)
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn get(&self, name: &str) -> Result<Option<RoleEntry>, Error> {
        self.pool
            .query(&format!(
                "SELECT sql, revocation_sql FROM {ROLES_TABLE} WHERE name = ?"
            ))?
            .bind(name)
            .fetch_optional()
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        path_roles::RoleEntry,
        store::{connection::tests::setup_context, role::RoleStore},
    };

    #[sqlx::test]
    async fn crud() {
        let pool = setup_context().await;
        let store = RoleStore::new(pool);

        let role_name = "foo";

        assert!(store.get(role_name).await.unwrap().is_none());

        let role = RoleEntry {
            sql: "SELECT ..".into(),
            revocation_sql: "UPDATE ..".into(),
        };
        assert!(store.create(role_name, &role).await.is_ok());
        assert_eq!(store.get(role_name).await.unwrap(), Some(role.clone()));
    }
}
