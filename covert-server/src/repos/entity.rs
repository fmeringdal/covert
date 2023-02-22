use std::sync::Arc;

use covert_storage::EncryptedPool;
use covert_types::entity::{Entity, EntityAlias};

use crate::error::Error;

pub struct EntityRepo {
    pool: Arc<EncryptedPool>,
}

impl Clone for EntityRepo {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
        }
    }
}

impl EntityRepo {
    pub fn new(pool: Arc<EncryptedPool>) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip(self))]
    pub async fn create(&self, entity: &Entity) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO ENTITIES (name, disabled, namespace_id)
            VALUES (?, ?, ?)",
        )
        .bind(&entity.name)
        .bind(entity.disabled)
        .bind(&entity.namespace_id)
        .execute(self.pool.as_ref())
        .await
        .map(|_| ())
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn attach_alias(
        &self,
        name: &str,
        alias: &EntityAlias,
        namespace_id: &str,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO ENTITY_ALIASES (name, mount_path, entity_name, namespace_id)
            VALUES (?, ?, ?, ?)",
        )
        .bind(&alias.name)
        .bind(&alias.mount_path)
        .bind(name)
        .bind(namespace_id)
        .execute(self.pool.as_ref())
        .await
        .map(|_| ())
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn attach_policy(
        &self,
        name: &str,
        policy: &str,
        namespace_id: &str,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO ENTITY_POLICIES (entity_name, policy_name, namespace_id)
            VALUES (?, ?, ?)",
        )
        .bind(name)
        .bind(policy)
        .bind(namespace_id)
        .execute(self.pool.as_ref())
        .await
        .map(|_| ())
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_policy(
        &self,
        name: &str,
        policy: &str,
        namespace_id: &str,
    ) -> Result<bool, Error> {
        sqlx::query(
            "DELETE FROM ENTITY_POLICIES WHERE
                entity_name = ? AND policy_name = ? AND namespace_id = ?",
        )
        .bind(name)
        .bind(policy)
        .bind(namespace_id)
        .execute(self.pool.as_ref())
        .await
        .map(|res| res.rows_affected() == 1)
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_alias(
        &self,
        name: &str,
        alias: &EntityAlias,
        namespace_id: &str,
    ) -> Result<bool, Error> {
        sqlx::query(
            "DELETE FROM ENTITY_ALIASES WHERE
                name = ? AND mount_path = ? AND entity_name = ? AND namespace_id = ?",
        )
        .bind(&alias.name)
        .bind(&alias.mount_path)
        .bind(name)
        .bind(namespace_id)
        .execute(self.pool.as_ref())
        .await
        .map(|res| res.rows_affected() == 1)
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_entity_from_alias(
        &self,
        alias: &EntityAlias,
        namespace_id: &str,
    ) -> Result<Option<Entity>, Error> {
        sqlx::query_as(
            "SELECT E.* FROM ENTITY_ALIASES EA 
                INNER JOIN ENTITIES E ON EA.entity_name = E.name AND EA.namespace_id = E.namespace_id
                WHERE EA.name = ? AND EA.mount_path = ? AND EA.namespace_id = ?",
        )
        .bind(&alias.name)
        .bind(&alias.mount_path)
        .bind(namespace_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use covert_types::{
        backend::BackendType,
        mount::{MountConfig, MountEntry},
        policy::{PathPolicy, Policy},
        request::Operation,
    };
    use uuid::Uuid;

    use crate::repos::{
        mount::{tests::pool, MountRepo},
        namespace::{Namespace, NamespaceRepo},
        policy::PolicyRepo,
    };

    use super::*;

    #[tokio::test]
    async fn crud() {
        let pool = Arc::new(pool().await);
        let policy_repo = Arc::new(PolicyRepo::new(Arc::clone(&pool)));
        let entity_repo = EntityRepo::new(Arc::clone(&pool));
        let lease_repo = MountRepo::new(Arc::clone(&pool));
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ns_repo.create(&ns).await.unwrap();

        // Create some policies
        let foo_policy = Policy::new(
            "foo".into(),
            vec![PathPolicy::new("foo/".into(), vec![Operation::Read])],
            ns.id.clone(),
        );
        policy_repo.create(&foo_policy).await.unwrap();
        let bar_policy = Policy::new(
            "bar".into(),
            vec![PathPolicy::new("bar/".into(), vec![Operation::Update])],
            ns.id.clone(),
        );
        policy_repo.create(&bar_policy).await.unwrap();

        let entity = Entity::new("John".into(), false, ns.id.clone());
        assert!(entity_repo.create(&entity).await.is_ok());

        // Attach "foo" policy to "John"
        assert!(entity_repo
            .attach_policy(entity.name(), foo_policy.name(), &ns.id)
            .await
            .is_ok());

        // Attach alias to "John" for mount "userpass"
        let userpass_mount = MountEntry {
            id: Uuid::new_v4(),
            backend_type: BackendType::Userpass,
            config: MountConfig::default(),
            path: "auth/".into(),
            namespace_id: ns.id.clone(),
        };
        lease_repo.create(&userpass_mount).await.unwrap();

        let alias = EntityAlias {
            name: "John-Alias".into(),
            mount_path: userpass_mount.path.clone(),
        };
        assert!(entity_repo
            .attach_alias(entity.name(), &alias, &ns.id)
            .await
            .is_ok());

        // Lookup entity by alias
        assert_eq!(
            entity_repo
                .get_entity_from_alias(&alias, &ns.id)
                .await
                .unwrap(),
            Some(entity.clone())
        );

        // Remove policy from entity
        assert!(entity_repo
            .remove_policy(entity.name(), foo_policy.name(), &ns.id)
            .await
            .unwrap());
        // Remove again fails
        assert!(!entity_repo
            .remove_policy(entity.name(), foo_policy.name(), &ns.id)
            .await
            .unwrap());

        // Remove alias from entity
        assert!(entity_repo
            .remove_alias(entity.name(), &alias, &ns.id)
            .await
            .unwrap());
        // Remove again fails
        assert!(!entity_repo
            .remove_alias(entity.name(), &alias, &ns.id)
            .await
            .unwrap());
        // Lookup entity by alias should now fail
        assert_eq!(
            entity_repo
                .get_entity_from_alias(&alias, &ns.id)
                .await
                .unwrap(),
            None
        );
    }
}
