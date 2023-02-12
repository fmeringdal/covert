use std::path::Path;

use covert_types::state::VaultState;
use futures::Stream;
use sqlx::{
    sqlite::{SqliteQueryResult, SqliteRow},
    Pool, Sqlite, Transaction,
};
use tempfile::TempDir;

use crate::{
    states::{Sealed, Uninitialized, Unsealed},
    storage::{create_ecrypted_pool, create_master_key, Storage},
    utils::owned_rw_lock::{OwnedRwLock, TransitionResult},
};

#[derive(Debug)]
pub struct EncryptedPool(OwnedRwLock<PoolState>);

struct PoolClosedStream;

impl Stream for PoolClosedStream {
    type Item = Result<sqlx::Either<SqliteQueryResult, SqliteRow>, sqlx::Error>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Ready(Some(Err(sqlx::Error::PoolClosed)))
    }
}

impl<'c> sqlx::Executor<'c> for &EncryptedPool {
    type Database = Sqlite;

    fn fetch_many<'e, 'q, E>(
        self,
        query: E,
    ) -> futures::stream::BoxStream<
        'e,
        Result<
            sqlx::Either<
                <Self::Database as sqlx::Database>::QueryResult,
                <Self::Database as sqlx::Database>::Row,
            >,
            sqlx::Error,
        >,
    >
    where
        'c: 'e,
        'q: 'e,
        E: 'q + sqlx::Execute<'q, Self::Database>,
    {
        let pool = match self.pool() {
            Ok(pool) => pool,
            Err(_) => return Box::pin(PoolClosedStream),
        };
        pool.fetch_many(query)
    }

    fn fetch_optional<'e, 'q, E>(
        self,
        query: E,
    ) -> futures::future::BoxFuture<
        'e,
        Result<Option<<Self::Database as sqlx::Database>::Row>, sqlx::Error>,
    >
    where
        'c: 'e,
        'q: 'e,
        E: 'q + sqlx::Execute<'q, Self::Database>,
    {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(err) => return Box::pin(async { Err(err) }),
        };
        pool.fetch_optional(query)
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [<Self::Database as sqlx::Database>::TypeInfo],
    ) -> futures::future::BoxFuture<
        'e,
        Result<<Self::Database as sqlx::database::HasStatement<'q>>::Statement, sqlx::Error>,
    >
    where
        'c: 'e,
    {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(err) => return Box::pin(async { Err(err) }),
        };
        pool.prepare_with(sql, parameters)
    }

    fn describe<'e, 'q: 'e>(
        self,
        sql: &'q str,
    ) -> futures::future::BoxFuture<'e, Result<sqlx::Describe<Self::Database>, sqlx::Error>>
    where
        'c: 'e,
    {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(err) => return Box::pin(async { Err(err) }),
        };
        pool.describe(sql)
    }
}

#[derive(Debug)]
pub enum PoolState {
    Uninitialized(Storage<Uninitialized>),
    Sealed(Storage<Sealed>),
    Unsealed(Storage<Unsealed>),
}

impl PoolState {
    /// Try to get a unsealed storage.
    ///
    /// # Errors
    ///
    /// Returns error if the storage is not unsealed.
    pub fn get_unsealed(&self) -> Result<&Storage<Unsealed>, EncryptedPoolError> {
        match self {
            PoolState::Uninitialized(_) => {
                Err(EncryptedPoolError::InvalidState(VaultState::Uninitialized))
            }
            PoolState::Sealed(_) => Err(EncryptedPoolError::InvalidState(VaultState::Sealed)),
            PoolState::Unsealed(b) => Ok(b),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EncryptedPoolError {
    #[error("This operation is not allowed when the current state is `{0}`")]
    InvalidState(VaultState),
    #[error("Failed to transition the pool state from `{from}` to `{to}`")]
    Transition { from: VaultState, to: VaultState },
}

impl EncryptedPool {
    pub fn new(storage_path: &impl ToString) -> Self {
        let storage_path = storage_path.to_string();

        if Path::new(&storage_path).exists() {
            Self(OwnedRwLock::new(PoolState::Sealed(Storage {
                state: Sealed,
                storage_path,
            })))
        } else {
            Self(OwnedRwLock::new(PoolState::Uninitialized(Storage {
                state: Uninitialized,
                storage_path,
            })))
        }
    }

    /// Creates an unsealed temporary pool which is useful when writing tests.
    #[must_use]
    pub fn new_tmp() -> Self {
        let tmpdir =
            TempDir::new().expect("tmp dir to be created and this should only be used for testing");
        let storage_path = tmpdir
            .path()
            .join("db-storage")
            .to_str()
            .expect("tmp dir should exist and this should only be used for testing")
            .to_string();
        let master_key = create_master_key();
        let pool = create_ecrypted_pool(true, &storage_path, master_key)
            .expect("to create encrypted pool and this should only be used for testing");

        Self(OwnedRwLock::new(PoolState::Unsealed(Storage {
            state: Unsealed {
                pool,
                tmpdir: Some(tmpdir),
            },
            storage_path,
        })))
    }

    pub fn state(&self) -> VaultState {
        #[allow(clippy::redundant_closure_for_method_calls)]
        self.0.map(|barrier| barrier.into())
    }

    /// Initialize the pool.
    ///
    /// # Errors
    ///
    /// Returns error if the pool is not uninitialized or the initialization fails.
    pub fn initialize(&self) -> Result<Option<String>, EncryptedPoolError> {
        self.0.write(|barrier| {
            let barrier = match barrier {
                PoolState::Uninitialized(barrier) => barrier,
                PoolState::Sealed(barrier) => {
                    return TransitionResult {
                        state: PoolState::Sealed(barrier),
                        result: Err(EncryptedPoolError::InvalidState(VaultState::Sealed)),
                    }
                }
                PoolState::Unsealed(barrier) => {
                    return TransitionResult {
                        state: PoolState::Unsealed(barrier),
                        result: Err(EncryptedPoolError::InvalidState(VaultState::Unsealed)),
                    }
                }
            };

            match barrier.initialize() {
                Ok(res) => TransitionResult {
                    state: PoolState::Sealed(res.sealed_storage),
                    result: Ok(res.master_key),
                },
                Err(barrier) => TransitionResult {
                    state: PoolState::Uninitialized(barrier),
                    result: Err(EncryptedPoolError::Transition {
                        from: VaultState::Uninitialized,
                        to: VaultState::Sealed,
                    }),
                },
            }
        })
    }

    /// Unseal the pool.
    ///
    /// # Errors
    ///
    /// Returns error if the pool is not sealed or the unseal process fails.
    pub fn unseal(&self, master_key: String) -> Result<(), EncryptedPoolError> {
        self.0.write(|barrier| {
            let barrier = match barrier {
                PoolState::Uninitialized(barrier) => {
                    return TransitionResult {
                        state: PoolState::Uninitialized(barrier),
                        result: Err(EncryptedPoolError::InvalidState(VaultState::Uninitialized)),
                    }
                }
                PoolState::Sealed(barrier) => barrier,
                PoolState::Unsealed(barrier) => {
                    return TransitionResult {
                        state: PoolState::Unsealed(barrier),
                        result: Err(EncryptedPoolError::InvalidState(VaultState::Unsealed)),
                    }
                }
            };

            match barrier.unseal(master_key) {
                Ok(barrier) => TransitionResult {
                    state: PoolState::Unsealed(barrier),
                    result: Ok(()),
                },
                Err(barrier) => TransitionResult {
                    state: PoolState::Sealed(barrier),
                    result: Err(EncryptedPoolError::Transition {
                        from: VaultState::Sealed,
                        to: VaultState::Unsealed,
                    }),
                },
            }
        })
    }

    /// Seal the pool.
    ///
    /// # Errors
    ///
    /// Returns error if the pool is not unsealed.
    pub fn seal(&self) -> Result<(), EncryptedPoolError> {
        self.0.write(|barrier| {
            let barrier = match barrier {
                PoolState::Uninitialized(barrier) => {
                    return TransitionResult {
                        state: PoolState::Uninitialized(barrier),
                        result: Err(EncryptedPoolError::InvalidState(VaultState::Uninitialized)),
                    }
                }
                PoolState::Sealed(barrier) => {
                    return TransitionResult {
                        state: PoolState::Sealed(barrier),
                        result: Err(EncryptedPoolError::InvalidState(VaultState::Sealed)),
                    }
                }
                PoolState::Unsealed(barrier) => barrier,
            };

            let barrier = barrier.seal();
            TransitionResult {
                state: PoolState::Sealed(barrier),
                result: Ok(()),
            }
        })
    }

    fn pool(&self) -> Result<Pool<Sqlite>, sqlx::Error> {
        self.0
            .read()
            .get_unsealed()
            .map(|storage| storage.state.pool.clone())
            .map_err(|_| sqlx::Error::PoolClosed)
    }

    /// Retrieves a connection and immediately begins a new transaction.
    ///
    /// # Errors
    ///
    /// Returns error if it is unable to retrieve the db pool or start the
    /// transaction.
    pub async fn begin(&self) -> Result<Transaction<'static, Sqlite>, sqlx::Error> {
        let pool = self.pool()?;
        pool.begin().await
    }
}

impl From<&PoolState> for VaultState {
    fn from(barrier: &PoolState) -> Self {
        match barrier {
            PoolState::Uninitialized(_) => VaultState::Uninitialized,
            PoolState::Sealed(_) => VaultState::Sealed,
            PoolState::Unsealed(_) => VaultState::Unsealed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn unseal_and_query() {
        let tmpdir = tempfile::tempdir().unwrap();
        let file_path = tmpdir
            .path()
            .join("db-storage")
            .to_str()
            .unwrap()
            .to_string();

        let pool = EncryptedPool::new(&file_path);

        const QUERY: &str = "SELECT count(*) FROM sqlite_master";

        let res = sqlx::query(QUERY).execute(&pool).await;
        assert!(matches!(res.unwrap_err(), sqlx::Error::PoolClosed));

        let master_key = pool.initialize().unwrap().unwrap();
        let res = sqlx::query(QUERY).execute(&pool).await;
        assert!(matches!(res.unwrap_err(), sqlx::Error::PoolClosed));

        // Unseal and we should get a success response
        pool.unseal(master_key.clone()).unwrap();
        let res = sqlx::query(QUERY).execute(&pool).await;
        assert!(res.is_ok());

        // Seal and we should not be able to query
        pool.seal().unwrap();
        let res = sqlx::query(QUERY).execute(&pool).await;
        assert!(matches!(res.unwrap_err(), sqlx::Error::PoolClosed));

        // Unseal again and we should get a success response
        pool.unseal(master_key).unwrap();
        let res = sqlx::query(QUERY).execute(&pool).await;
        assert!(res.is_ok());
    }
}
