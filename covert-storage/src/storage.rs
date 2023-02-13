use std::path::Path;

use rand::Rng;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Pool, Sqlite,
};

use crate::states::{Sealed, Uninitialized, Unsealed};

#[derive(Debug)]
pub struct Storage<T> {
    pub state: T,
    pub storage_path: String,
}

pub struct InitializeResult {
    pub sealed_storage: Storage<Sealed>,
    pub master_key: Option<String>,
}

impl Storage<Uninitialized> {
    pub fn new(storage_path: String) -> Self {
        Storage {
            state: Uninitialized,
            storage_path,
        }
    }

    pub fn initialize(self) -> Result<InitializeResult, Storage<Uninitialized>> {
        // Check if path exists
        if Path::new(&self.storage_path).exists() {
            Ok(InitializeResult {
                sealed_storage: Storage {
                    state: Sealed,
                    storage_path: self.storage_path,
                },
                master_key: None,
            })
        } else {
            let master_key = create_master_key();

            // otherwise create master key, db file and return
            create_ecrypted_pool(true, &self.storage_path, master_key.clone())
                .map(|_| InitializeResult {
                    sealed_storage: Storage {
                        state: Sealed,
                        storage_path: self.storage_path.clone(),
                    },
                    master_key: Some(master_key),
                })
                .map_err(|_| self)
        }
    }
}

impl Storage<Sealed> {
    pub fn unseal(self, key: String) -> Result<Storage<Unsealed>, Self> {
        create_ecrypted_pool(false, &self.storage_path, key)
            .map(|pool| Storage {
                state: Unsealed { pool },
                storage_path: self.storage_path.clone(),
            })
            .map_err(|_| self)
    }
}

impl Storage<Unsealed> {
    pub fn seal(self) -> Storage<Sealed> {
        Storage {
            state: Sealed,
            storage_path: self.storage_path,
        }
    }
}

const ALPHABET: [char; 52] = [
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
    'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
];

pub(crate) fn create_master_key() -> String {
    let mut rng = rand::thread_rng();

    let mut key = Vec::with_capacity(50);

    for _ in 0..50 {
        let idx = rng.gen_range(0..ALPHABET.len());
        key.push(ALPHABET[idx]);
    }

    key.iter().collect()
}

pub(crate) fn create_ecrypted_pool(
    create_if_missing: bool,
    storage_path: &str,
    key: String,
) -> Result<Pool<Sqlite>, sqlx::Error> {
    let opts = SqliteConnectOptions::new()
        .create_if_missing(create_if_missing)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .synchronous(SqliteSynchronous::Full)
        .pragma("key", key)
        .filename(storage_path);

    let (tx, rx) = std::sync::mpsc::channel();

    futures::executor::block_on(async move {
        async fn connect_and_verify(
            opts: SqliteConnectOptions,
        ) -> Result<Pool<Sqlite>, sqlx::Error> {
            let pool = SqlitePoolOptions::new()
                // TODO: allow configuration of these values
                .min_connections(1)
                .max_connections(1)
                .connect_with(opts)
                .await?;

            // Verify key
            sqlx::query("SELECT count(*) FROM sqlite_master")
                .execute(&pool)
                .await?;

            Ok(pool)
        }
        let res = connect_and_verify(opts).await;
        if tx.send(res).is_err() {
            tracing::error!("Unable to send connection verify message");
        }
    });

    let pool = rx.recv().map_err(|_| sqlx::Error::PoolTimedOut)??;

    Ok(pool)
}
