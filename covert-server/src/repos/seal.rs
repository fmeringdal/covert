use aes_gcm::{
    aead::{Aead, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use rand::distributions::{Alphanumeric, DistString};
use sqlx::{Pool, Sqlite};

use crate::error::{Error, ErrorType};

const SEAL_CONFIGURATION_TABLE: &str = "SEAL_CONFIG";

const KEY_SHARES_TABLE: &str = "KEY_SHARES";

#[derive(Debug, sqlx::FromRow, PartialEq, Eq)]
pub struct SealConfig {
    pub threshold: u8,
    pub shares: u8,
}

#[derive(sqlx::FromRow)]
pub struct KeyShare {
    pub key: Vec<u8>,
    pub nonce: Vec<u8>,
}

#[derive(Clone)]
pub struct SealRepo {
    pool: Pool<Sqlite>,
    cipher: Aes256Gcm,
}

impl SealRepo {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        let encryption_key = Aes256Gcm::generate_key(&mut OsRng);
        let cipher = Aes256Gcm::new(&encryption_key);

        Self { pool, cipher }
    }

    pub async fn set_config(&self, config: &SealConfig) -> Result<(), Error> {
        sqlx::query(&format!(
            "INSERT INTO {SEAL_CONFIGURATION_TABLE} (shares, threshold, lock) 
                    VALUES ($1, $2, $3)"
        ))
        .bind(config.shares)
        .bind(config.threshold)
        .bind(1)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(Into::into)
    }

    pub async fn get_config(&self) -> Result<Option<SealConfig>, Error> {
        sqlx::query_as(&format!("SELECT * FROM {SEAL_CONFIGURATION_TABLE}"))
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn clear_key_shares(&self) -> Result<u64, Error> {
        sqlx::query(&format!("DELETE FROM {KEY_SHARES_TABLE}"))
            .execute(&self.pool)
            .await
            .map(|res| res.rows_affected())
            .map_err(Into::into)
    }

    pub async fn insert_key_share(&self, key: &[u8]) -> Result<(), Error> {
        let nonce = random_nonce();
        let nonce = nonce.as_bytes();
        let nonce_array = Nonce::from_slice(nonce);

        let key = self.cipher.encrypt(nonce_array, key).map_err(|_| {
            ErrorType::InternalError(anyhow::Error::msg("Unable to encrypt key share"))
        })?;

        sqlx::query(&format!(
            "INSERT INTO {KEY_SHARES_TABLE} (key, nonce) VALUES ($1, $2)"
        ))
        .bind(key)
        .bind(nonce)
        .execute(&self.pool)
        .await
        .map_err(Into::into)
        .map(|_| ())
    }

    pub async fn get_key_shares(&self) -> Result<Vec<KeyShare>, Error> {
        let key_shares: Vec<KeyShare> =
            sqlx::query_as(&format!("SELECT * FROM {KEY_SHARES_TABLE}"))
                .fetch_all(&self.pool)
                .await?;

        let mut decrypted_key_shares: Vec<KeyShare> = vec![];
        for key_share in key_shares {
            let nonce = Nonce::from_slice(&key_share.nonce);

            let Ok(decrypted_key) = self.cipher.decrypt(nonce, key_share.key.as_ref()) else {
                // Clear all shares if there are any bad shares
                self.clear_key_shares().await?;

                return Err(ErrorType::BadData(
                    "Unable to decrypt key share from seal storage".into(),
                ))?;
            };
            if !decrypted_key_shares.iter().any(|k| k.key == decrypted_key) {
                decrypted_key_shares.push(KeyShare {
                    key: decrypted_key,
                    nonce: key_share.nonce,
                });
            }
        }

        Ok(decrypted_key_shares)
    }
}

fn random_nonce() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 12)
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use super::*;

    #[tokio::test]
    async fn crud() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        crate::migrations::migrate_unecrypted_db(&pool)
            .await
            .unwrap();
        let seal = SealRepo::new(pool);

        assert!(seal.get_config().await.unwrap().is_none());

        let config = SealConfig {
            shares: 5,
            threshold: 3,
        };

        assert!(seal.set_config(&config).await.is_ok());
        assert_eq!(seal.get_config().await.unwrap(), Some(config));

        assert!(seal.get_key_shares().await.unwrap().is_empty());

        let key_share_1 = "my-secret-share-1";
        assert!(seal.insert_key_share(key_share_1.as_bytes()).await.is_ok());
        // Duplicate key shares can be inserted
        for _ in 0..5 {
            assert!(seal.insert_key_share(key_share_1.as_bytes()).await.is_ok());
        }

        // Get key shares will only return unique keys
        let key_shares = seal.get_key_shares().await.unwrap();
        assert_eq!(key_shares.len(), 1);
        assert_eq!(key_shares[0].key, key_share_1.as_bytes());

        let key_share_2 = "my-secret-share-2";
        assert!(seal.insert_key_share(key_share_2.as_bytes()).await.is_ok());

        let key_shares = seal.get_key_shares().await.unwrap();
        assert_eq!(key_shares.len(), 2);
        assert_eq!(key_shares[0].key, key_share_1.as_bytes());
        assert_eq!(key_shares[1].key, key_share_2.as_bytes());

        assert!(seal.clear_key_shares().await.is_ok());
        assert!(seal.get_key_shares().await.unwrap().is_empty());
    }
}
