use std::sync::Arc;

use covert_storage::EncryptedPool;

use crate::error::Error;

pub const NAMESPACE_TABLE: &str = "NAMESPACES";

pub struct NamespaceRepo {
    pool: Arc<EncryptedPool>,
}

impl Clone for NamespaceRepo {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, sqlx::FromRow)]
pub struct Namespace {
    pub id: String,
    pub name: String,
    pub parent_namespace_id: Option<String>,
}

impl NamespaceRepo {
    pub fn new(pool: Arc<EncryptedPool>) -> Self {
        Self { pool }
    }

    #[tracing::instrument(skip(self))]
    pub async fn create(&self, ns: &Namespace) -> Result<(), Error> {
        sqlx::query(&format!(
            "INSERT INTO {NAMESPACE_TABLE} (id, name, parent_namespace_id)
                VALUES (?, ?, ?)"
        ))
        .bind(&ns.id)
        .bind(&ns.name)
        .bind(ns.parent_namespace_id.as_ref())
        .execute(self.pool.as_ref())
        .await
        .map(|_| ())
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn lookup(&self, namespace_id: &str) -> Result<Option<Namespace>, Error> {
        sqlx::query_as(&format!("SELECT * FROM {NAMESPACE_TABLE} WHERE id = ?"))
            .bind(namespace_id)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(Into::into)
    }

    // Try to delete namespace. Will fail if called on root namespace
    #[tracing::instrument(skip(self))]
    pub async fn delete(&self, namespace_id: &str) -> Result<bool, Error> {
        sqlx::query(&format!(
            "DELETE FROM {NAMESPACE_TABLE} WHERE id = ? AND parent_namespace_id IS NOT NULL"
        ))
        .bind(namespace_id)
        .execute(self.pool.as_ref())
        .await
        .map(|res| res.rows_affected() == 1)
        .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn find_by_path(&self, path: &[String]) -> Result<Option<Namespace>, Error> {
        if path.is_empty() {
            return Ok(None);
        }

        let mut query = format!(
            "SELECT NS{}.* FROM {NAMESPACE_TABLE} AS NS0",
            path.len() - 1
        );
        for idx in 1..path.len() {
            let parent_idx = idx - 1;
            query = format!("{query} INNER JOIN {NAMESPACE_TABLE} AS NS{idx} ON NS{idx}.parent_namespace_id = NS{parent_idx}.id");
        }
        query = format!("{query} WHERE");
        for idx in 1..path.len() {
            query = format!("{query} NS{idx}.name = ${} AND", idx + 1);
        }
        query = format!("{query} NS0.parent_namespace_id IS NULL");
        let mut query = sqlx::query_as(&query);
        for name in path {
            query = query.bind(name);
        }
        query
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    pub async fn find_parents(&self, id: &str) -> Result<Vec<Namespace>, Error> {
        let mut parents = vec![];
        let Some(ns) = sqlx::query_as::<_, Namespace>(&format!(
            "SELECT * FROM {NAMESPACE_TABLE} WHERE id = ?"
        ))
        .bind(id.to_string())
        .fetch_optional(self.pool.as_ref())
        .await?
        else {
            return Ok(vec![]);
        };

        let mut parent_namespace_id = ns.parent_namespace_id.clone();
        parents.push(ns);

        while let Some(parent_id) = parent_namespace_id {
            let ns = sqlx::query_as(&format!("SELECT * FROM {NAMESPACE_TABLE} WHERE id = ?"))
                .bind(parent_id.to_string())
                .fetch_optional(self.pool.as_ref())
                .await?;
            parent_namespace_id = ns
                .as_ref()
                .and_then(|ns: &Namespace| ns.parent_namespace_id.clone());
            if let Some(ns) = ns {
                parents.push(ns);
            }
        }

        Ok(parents)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_full_path(&self, id: &str) -> Result<String, Error> {
        self.find_parents(id).await.map(|parents| {
            parents
                .into_iter()
                .rev()
                .map(|ns| ns.name)
                .collect::<Vec<_>>()
                .join("/")
        })
    }

    #[tracing::instrument(skip(self))]
    pub async fn list(&self, id: &str) -> Result<Vec<Namespace>, Error> {
        sqlx::query_as(&format!(
            "SELECT * FROM {NAMESPACE_TABLE} WHERE parent_namespace_id = ? ORDER BY name ASC"
        ))
        .bind(id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::repos::mount::tests::pool;

    use super::*;

    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn crud() {
        let pool = Arc::new(pool().await);
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let root_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ns_repo.create(&root_ns).await.unwrap();
        let root_foo_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "foo".to_string(),
            parent_namespace_id: Some(root_ns.id.clone()),
        };
        ns_repo.create(&root_foo_ns).await.unwrap();
        let root_bar_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "bar".to_string(),
            parent_namespace_id: Some(root_ns.id.clone()),
        };
        ns_repo.create(&root_bar_ns).await.unwrap();
        let root_foo_bar_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "bar".to_string(),
            parent_namespace_id: Some(root_foo_ns.id.clone()),
        };
        ns_repo.create(&root_foo_bar_ns).await.unwrap();
        let root_foo_bar_baz_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "baz".to_string(),
            parent_namespace_id: Some(root_foo_bar_ns.id.clone()),
        };
        ns_repo.create(&root_foo_bar_baz_ns).await.unwrap();

        // List child namespaces
        assert_eq!(
            ns_repo.list(&root_ns.id).await.unwrap(),
            vec![root_bar_ns.clone(), root_foo_ns.clone()]
        );
        assert_eq!(
            ns_repo.list(&root_foo_ns.id).await.unwrap(),
            vec![root_foo_bar_ns.clone()]
        );
        assert!(ns_repo
            .list(&root_foo_bar_baz_ns.id)
            .await
            .unwrap()
            .is_empty());

        // Find by path
        assert!(ns_repo.find_by_path(&[]).await.unwrap().is_none());

        assert_eq!(
            ns_repo.find_by_path(&[root_ns.name.clone()]).await.unwrap(),
            Some(root_ns.clone())
        );
        assert_eq!(
            ns_repo
                .find_by_path(&[root_ns.name.clone(), root_foo_ns.name.clone()])
                .await
                .unwrap(),
            Some(root_foo_ns.clone())
        );
        assert_eq!(
            ns_repo
                .find_by_path(&[root_ns.name.clone(), root_bar_ns.name.clone()])
                .await
                .unwrap(),
            Some(root_bar_ns.clone())
        );
        assert_eq!(
            ns_repo
                .find_by_path(&[
                    root_ns.name.clone(),
                    root_foo_ns.name.clone(),
                    root_foo_bar_ns.name.clone()
                ])
                .await
                .unwrap(),
            Some(root_foo_bar_ns.clone())
        );
        assert_eq!(
            ns_repo
                .find_by_path(&[
                    root_ns.name.clone(),
                    root_foo_ns.name.clone(),
                    root_foo_bar_ns.name.clone(),
                    root_foo_bar_baz_ns.name.clone()
                ])
                .await
                .unwrap(),
            Some(root_foo_bar_baz_ns.clone())
        );

        // Find parents and get by full path
        assert_eq!(
            ns_repo
                .find_parents(&Uuid::new_v4().to_string())
                .await
                .unwrap(),
            vec![]
        );
        assert_eq!(
            ns_repo.find_parents(&root_ns.id).await.unwrap(),
            vec![root_ns.clone()]
        );
        assert_eq!(
            ns_repo.get_full_path(&root_ns.id).await.unwrap(),
            root_ns.name.clone()
        );
        assert_eq!(
            ns_repo.find_parents(&root_foo_ns.id).await.unwrap(),
            vec![root_foo_ns.clone(), root_ns.clone()]
        );
        assert_eq!(
            ns_repo.get_full_path(&root_foo_ns.id).await.unwrap(),
            format!("{}/{}", root_ns.name.clone(), root_foo_ns.name.clone())
        );
        assert_eq!(
            ns_repo.find_parents(&root_bar_ns.id).await.unwrap(),
            vec![root_bar_ns.clone(), root_ns.clone()]
        );
        assert_eq!(
            ns_repo.get_full_path(&root_bar_ns.id).await.unwrap(),
            format!("{}/{}", root_ns.name.clone(), root_bar_ns.name.clone())
        );
        assert_eq!(
            ns_repo.find_parents(&root_foo_bar_baz_ns.id).await.unwrap(),
            vec![
                root_foo_bar_baz_ns.clone(),
                root_foo_bar_ns.clone(),
                root_foo_ns.clone(),
                root_ns.clone(),
            ]
        );
        assert_eq!(
            ns_repo
                .get_full_path(&root_foo_bar_baz_ns.id)
                .await
                .unwrap(),
            format!(
                "{}/{}/{}/{}",
                root_ns.name.clone(),
                root_foo_ns.name.clone(),
                root_foo_bar_ns.name.clone(),
                root_foo_bar_baz_ns.name.clone()
            )
        );
    }

    #[tokio::test]
    async fn no_slash_in_name() {
        let pool = Arc::new(pool().await);
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let root_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ns_repo.create(&root_ns).await.unwrap();

        for name in ["/", "/foo", "fo/o", "foo/"] {
            let foo_slash_ns = Namespace {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                parent_namespace_id: Some(root_ns.id.clone()),
            };
            assert!(ns_repo.create(&foo_slash_ns).await.is_err());
        }
    }

    #[tokio::test]
    async fn reject_empty_name() {
        let pool = Arc::new(pool().await);
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let root_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ns_repo.create(&root_ns).await.unwrap();
        let foo_slash_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: String::new(),
            parent_namespace_id: Some(root_ns.id.clone()),
        };
        assert!(ns_repo.create(&foo_slash_ns).await.is_err());
    }

    #[tokio::test]
    async fn no_space_in_name() {
        let pool = Arc::new(pool().await);
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let root_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        ns_repo.create(&root_ns).await.unwrap();

        for name in [" ", "s ", " s", " s ", "foo o"] {
            let foo_slash_ns = Namespace {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                parent_namespace_id: Some(root_ns.id.clone()),
            };
            assert!(ns_repo.create(&foo_slash_ns).await.is_err());
        }
    }

    #[tokio::test]
    async fn only_root_ns_is_orphan() {
        let pool = Arc::new(pool().await);
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let root_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "notroot".to_string(),
            parent_namespace_id: None,
        };
        assert!(ns_repo.create(&root_ns).await.is_err());
    }

    #[tokio::test]
    async fn unique_names_for_given_parent() {
        let pool = Arc::new(pool().await);
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let root_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        assert!(ns_repo.create(&root_ns).await.is_ok());

        // Trying to insert root again should fail
        let root_ns_2 = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        assert!(ns_repo.create(&root_ns_2).await.is_err());

        // Insert foo under root
        let foo_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "foo".to_string(),
            parent_namespace_id: Some(root_ns.id.clone()),
        };
        assert!(ns_repo.create(&foo_ns).await.is_ok());

        // Trying to insert another foo under root fails
        let foo_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "foo".to_string(),
            parent_namespace_id: Some(root_ns.id.clone()),
        };
        assert!(ns_repo.create(&foo_ns).await.is_err());
    }

    #[tokio::test]
    async fn cannot_use_root_as_sub_namespace() {
        let pool = Arc::new(pool().await);
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let root_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        assert!(ns_repo.create(&root_ns).await.is_ok());

        // Trying to insert root under root should not work
        let root_ns_2 = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: Some(root_ns.id.clone()),
        };
        assert!(ns_repo.create(&root_ns_2).await.is_err());

        // Insert foo under root
        let foo_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "foo".to_string(),
            parent_namespace_id: Some(root_ns.id.clone()),
        };
        assert!(ns_repo.create(&foo_ns).await.is_ok());

        // Trying to insert root under foo should not work
        let root_ns_2 = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: Some(foo_ns.id.clone()),
        };
        assert!(ns_repo.create(&root_ns_2).await.is_err());
    }

    #[tokio::test]
    async fn delete() {
        let pool = Arc::new(pool().await);
        let ns_repo = NamespaceRepo::new(Arc::clone(&pool));

        let root_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "root".to_string(),
            parent_namespace_id: None,
        };
        assert!(ns_repo.create(&root_ns).await.is_ok());

        // Insert foo under root
        let foo_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "foo".to_string(),
            parent_namespace_id: Some(root_ns.id.clone()),
        };
        assert!(ns_repo.create(&foo_ns).await.is_ok());

        // Insert bar under foo
        let bar_ns = Namespace {
            id: Uuid::new_v4().to_string(),
            name: "bar".to_string(),
            parent_namespace_id: Some(foo_ns.id.clone()),
        };
        assert!(ns_repo.create(&bar_ns).await.is_ok());

        // Delete root namespace does not work
        assert!(!ns_repo.delete(&root_ns.id).await.unwrap());

        // Trying to delete foo should fail as it has a child
        assert!(ns_repo.delete(&foo_ns.id).await.is_err());

        // Delete bar works as it has no child namespaces
        assert!(ns_repo.delete(&bar_ns.id).await.unwrap());
        // Delete again returns false
        assert!(!ns_repo.delete(&bar_ns.id).await.unwrap());

        // Delete foo now works as it has no child namespaces
        assert!(ns_repo.delete(&foo_ns.id).await.unwrap());

        // Delete root namespace does not work even without and childs
        assert!(!ns_repo.delete(&root_ns.id).await.unwrap());
    }
}
