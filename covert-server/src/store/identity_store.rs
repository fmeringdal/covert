use std::sync::Arc;

use covert_storage::EncryptedPool;
use covert_types::entity::{Entity, EntityAlias};

use crate::error::Error;

/// Store the identities known to Vault
pub struct IdentityStore {
    pool: Arc<EncryptedPool>,
}

impl IdentityStore {
    pub fn new(pool: Arc<EncryptedPool>) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip(self))]
    pub async fn create(&self, entity: &Entity) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO ENTITIES (name, disabled)
            VALUES (?, ?)",
        )
        .bind(entity.name())
        .bind(entity.disabled)
        .execute(self.pool.as_ref())
        .await
        .map(|_| ())
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn attach_alias(&self, name: &str, alias: &EntityAlias) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO ENTITY_ALIASES (name, mount_path, entity_name)
            VALUES (?, ?, ?)",
        )
        .bind(&alias.name)
        .bind(&alias.mount_path)
        .bind(name)
        .execute(self.pool.as_ref())
        .await
        .map(|_| ())
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn attach_policy(&self, name: &str, policy: &str) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO ENTITY_POLICIES (entity_name, policy_name)
            VALUES (?, ?)",
        )
        .bind(name)
        .bind(policy)
        .execute(self.pool.as_ref())
        .await
        .map(|_| ())
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_policy(&self, name: &str, policy: &str) -> Result<bool, Error> {
        sqlx::query(
            "DELETE FROM ENTITY_POLICIES WHERE
                entity_name = ? AND policy_name = ?",
        )
        .bind(name)
        .bind(policy)
        .execute(self.pool.as_ref())
        .await
        .map(|res| res.rows_affected() == 1)
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_alias(&self, name: &str, alias: &EntityAlias) -> Result<bool, Error> {
        sqlx::query(
            "DELETE FROM ENTITY_ALIASES WHERE
                name = ? AND mount_path = ? AND entity_name = ?",
        )
        .bind(&alias.name)
        .bind(&alias.mount_path)
        .bind(name)
        .execute(self.pool.as_ref())
        .await
        .map(|res| res.rows_affected() == 1)
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_entity_from_alias(
        &self,
        alias: &EntityAlias,
    ) -> Result<Option<Entity>, Error> {
        sqlx::query_as(
            "SELECT E.name, E.disabled FROM ENTITY_ALIASES EA 
                INNER JOIN ENTITIES E ON EA.entity_name = E.name
                WHERE EA.name = ? AND EA.mount_path = ?",
        )
        .bind(&alias.name)
        .bind(&alias.mount_path)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use covert_types::{
        backend::BackendType,
        mount::MountEntry,
        policy::{PathPolicy, Policy},
        request::Operation,
    };
    use uuid::Uuid;

    use crate::store::{
        mount_store::{tests::pool, MountStore},
        policy_store::PolicyStore,
    };

    use super::*;

    #[tokio::test]
    async fn crud() {
        let pool = Arc::new(pool().await.pool);
        let policy_store = Arc::new(PolicyStore::new(Arc::clone(&pool)));
        let identity_store = IdentityStore::new(Arc::clone(&pool));
        let mount_store = MountStore::new(Arc::clone(&pool));

        // Create some policies
        let foo_policy = Policy::new(
            "foo".into(),
            vec![PathPolicy::new("foo/".into(), vec![Operation::Read])],
        );
        policy_store.create(&foo_policy).await.unwrap();
        let bar_policy = Policy::new(
            "bar".into(),
            vec![PathPolicy::new("bar/".into(), vec![Operation::Update])],
        );
        policy_store.create(&bar_policy).await.unwrap();

        let entity = Entity::new("John".into(), false);
        assert!(identity_store.create(&entity).await.is_ok());

        // Attach "foo" policy to "John"
        assert!(identity_store
            .attach_policy(entity.name(), foo_policy.name())
            .await
            .is_ok());

        // Attach alias to "John" for mount "userpass"
        let userpass_mount = MountEntry {
            uuid: Uuid::new_v4(),
            backend_type: BackendType::Userpass,
            config: Default::default(),
            path: "auth/".into(),
        };
        mount_store.create(&userpass_mount).await.unwrap();

        let alias = EntityAlias {
            name: "John-Alias".into(),
            mount_path: userpass_mount.path.clone(),
        };
        assert!(identity_store
            .attach_alias(entity.name(), &alias)
            .await
            .is_ok());

        // Lookup entity by alias
        assert_eq!(
            identity_store.get_entity_from_alias(&alias).await.unwrap(),
            Some(entity.clone())
        );

        // Remove policy from entity
        assert!(identity_store
            .remove_policy(entity.name(), foo_policy.name())
            .await
            .unwrap());
        // Remove again fails
        assert!(!identity_store
            .remove_policy(entity.name(), foo_policy.name())
            .await
            .unwrap());

        // Remove alias from entity
        assert!(identity_store
            .remove_alias(entity.name(), &alias)
            .await
            .unwrap());
        // Remove again fails
        assert!(!identity_store
            .remove_alias(entity.name(), &alias)
            .await
            .unwrap());
        // Lookup entity by alias should now fail
        assert_eq!(
            identity_store.get_entity_from_alias(&alias).await.unwrap(),
            None
        );
    }
}
