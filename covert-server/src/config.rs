use serde::Deserialize;
use tokio::sync::oneshot;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub port: u16,
    #[serde(skip)]
    pub port_tx: Option<oneshot::Sender<u16>>,
    pub replication: Option<ReplicationConfig>,
    pub storage_path: String,
}

impl Config {
    #[must_use]
    pub fn seal_storage_path(&self) -> String {
        if self.using_inmemory_storage() {
            self.storage_path.to_string()
        } else {
            let maybe_slash = if self.storage_path.ends_with('/') {
                ""
            } else {
                "/"
            };
            format!("{}{maybe_slash}{}", self.storage_path, "seal.db")
        }
    }

    #[must_use]
    pub fn encrypted_storage_path(&self) -> String {
        if self.using_inmemory_storage() {
            self.storage_path.to_string()
        } else {
            let maybe_slash = if self.storage_path.ends_with('/') {
                ""
            } else {
                "/"
            };
            format!("{}{maybe_slash}{}", self.storage_path, "covert.db")
        }
    }

    #[must_use]
    pub fn using_inmemory_storage(&self) -> bool {
        self.storage_path.contains(":memory:")
    }

    pub fn sanitize(&self) -> anyhow::Result<()> {
        if self.replication.is_some() {
            if self.using_inmemory_storage() {
                return Err(anyhow::Error::msg(
                    "Replication is not supported for inmemory storage",
                ));
            }
        }

        if !self.using_inmemory_storage() {
            let storage_path = std::path::Path::new(&self.storage_path);
            if !storage_path.exists()
                && std::fs::DirBuilder::new()
                    .recursive(true)
                    .create(storage_path)
                    .is_err()
            {
                return Err(anyhow::Error::msg("Failed to create storage directory"));
            }

            if !storage_path.is_dir() {
                return Err(anyhow::Error::msg(
                    "The storage path provided is not a directory",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ReplicationConfig {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub endpoint: Option<String>,
    pub region: String,
    pub prefix: String,
    pub bucket: String,
    // TODO
    // S3 url format: https://<bucket-name>.s3.<region-code>.amazonaws.com/
    // pub bucket_url: String,
}

impl ReplicationConfig {
    #[must_use]
    pub fn seal_db_prefix(&self) -> String {
        let maybe_slash = if self.prefix.ends_with('/') { "" } else { "/" };
        format!("{}{maybe_slash}{}", self.prefix, "seal.db")
    }

    #[must_use]
    pub fn encrypted_db_prefix(&self) -> String {
        let maybe_slash = if self.prefix.ends_with('/') { "" } else { "/" };
        format!("{}{maybe_slash}{}", self.prefix, "covert.db")
    }
}
