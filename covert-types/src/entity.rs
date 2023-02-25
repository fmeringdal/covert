use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq, Clone)]
pub struct Entity {
    /// Entity name
    pub name: String,
    /// Namespace
    pub namespace_id: String,
}

impl Entity {
    #[must_use]
    pub fn new(name: String, namespace_id: String) -> Self {
        Self { name, namespace_id }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EntityAlias {
    pub name: String,
    pub mount_path: String,
}
