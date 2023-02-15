use covert_storage::BackendStoragePool;

use crate::{domain::config::Configuration, error::Error};

const CONFIGURATION_TABLE: &str = "CONFIG";

#[derive(Debug)]
pub struct Repo {
    pool: BackendStoragePool,
}

impl Repo {
    pub fn new(pool: BackendStoragePool) -> Self {
        Self { pool }
    }

    pub async fn load(&self) -> Result<Configuration, Error> {
        let res = self
            .pool
            .query(&format!("SELECT * FROM {CONFIGURATION_TABLE}"))?
            .fetch_optional()
            .await?;
        if let Some(config) = res {
            Ok(config)
        } else {
            let config = Configuration::default();
            self.set(&config).await?;
            Ok(config)
        }
    }

    pub async fn set(&self, config: &Configuration) -> Result<(), Error> {
        self.pool
            .query(&format!(
                "INSERT OR REPLACE INTO {CONFIGURATION_TABLE} (max_versions, lock) 
                    VALUES ($1, 1)"
            ))?
            .bind(config.max_versions)
            .execute()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use crate::store::secrets::tests::setup;

    #[sqlx::test]
    fn load_and_set_config() {
        let ctx = setup().await;
        let repo = &ctx.repos.config;

        let mut config = repo.load().await.unwrap();
        assert_eq!(config, Default::default());

        config.max_versions += 1;
        repo.set(&config).await.unwrap();

        let loaded_config = repo.load().await.unwrap();
        assert_eq!(config, loaded_config);
    }
}
