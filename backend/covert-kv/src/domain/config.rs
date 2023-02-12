use serde::{self, Deserialize, Serialize};

// defaultMaxVersions is the number of versions to keep around unless set by
// the config or key configuration.
pub const DEFAULT_MAX_VERSIONS: u32 = 10;

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow, PartialEq, Eq)]
pub struct Configuration {
    pub max_versions: u32,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            max_versions: DEFAULT_MAX_VERSIONS,
        }
    }
}
