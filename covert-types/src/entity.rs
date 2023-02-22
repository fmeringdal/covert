use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq, Clone)]
pub struct Entity {
    /// Entity name
    pub name: String,
    /// Set to true if entity is disabled from performing any action
    pub disabled: bool,
    /// Namespace
    pub namespace_id: String,
}

impl Entity {
    #[must_use]
    pub fn new(name: String, disabled: bool, namespace_id: String) -> Self {
        Self {
            name,
            disabled,
            namespace_id,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EntityAlias {
    pub name: String,
    pub mount_path: String,
}
