use std::{borrow::Cow, sync::Arc};

use sqlx::{
    error::DatabaseError,
    sqlite::{SqliteArguments, SqliteQueryResult, SqliteRow},
    Arguments, Encode, Sqlite, Type,
};

use crate::{scoped_queries::ScopedQuery, EncryptedPool};

#[derive(Debug, thiserror::Error)]
pub enum CovertDatabaseError {
    #[error("Unable to prefix query: `{query}` with prefix: `{prefix}`")]
    BadPrefixQuery {
        prefix: String,
        query: String,
        message: String,
    },
}

impl DatabaseError for CovertDatabaseError {
    #[inline]
    fn code(&self) -> Option<Cow<'_, str>> {
        // The SQLITE_ERROR result code is a generic error code that is used when no other more specific error code is available.
        Some("1".into())
    }

    fn message(&self) -> &str {
        match self {
            CovertDatabaseError::BadPrefixQuery { message, .. } => message,
        }
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }
}

#[derive(Debug, Clone)]
pub struct BackendStoragePool {
    prefix: String,
    pool: Arc<EncryptedPool>,
}

impl BackendStoragePool {
    pub fn new(prefix: &str, pool: Arc<EncryptedPool>) -> Self {
        Self {
            prefix: prefix.to_string(),
            pool,
        }
    }

    /// Construct a prefixed query.
    ///
    /// # Errors
    ///
    /// Returns error if the sql query cannot be prefixed.
    pub fn query(&self, sql: &impl ToString) -> Result<Query, sqlx::Error> {
        ScopedQuery::new(&self.prefix, &sql.to_string())
            .map(|query| Query {
                query,
                pool: Arc::clone(&self.pool),
                arguments: SqliteArguments::default(),
            })
            .map_err(|err| {
                let prefix = self.prefix.clone();
                let query = sql.to_string();
                let message = format!(
                    "Unable to prefix query: `{query}` with prefix: `{prefix}`. Error: {err:?}"
                );
                sqlx::Error::Database(Box::new(CovertDatabaseError::BadPrefixQuery {
                    prefix,
                    query,
                    message,
                }))
            })
    }

    // TODO: remove this
    pub fn query_no_prefix(&self, sql: &impl ToString) -> Query {
        Query {
            query: ScopedQuery {
                sql: sql.to_string(),
            },
            pool: Arc::clone(&self.pool),
            arguments: SqliteArguments::default(),
        }
    }

    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
}

pub struct Query<'a> {
    query: ScopedQuery,
    pool: Arc<EncryptedPool>,
    arguments: SqliteArguments<'a>,
}

impl<'a> Query<'a> {
    pub fn bind<T: 'a + Send + Encode<'a, Sqlite> + Type<Sqlite>>(mut self, value: T) -> Self {
        self.arguments.add(value);

        self
    }

    pub async fn execute(self) -> Result<SqliteQueryResult, sqlx::Error> {
        sqlx::query_with(self.query.sql(), self.arguments)
            .execute(self.pool.as_ref())
            .await
    }

    pub async fn fetch_one<T>(self) -> Result<T, sqlx::Error>
    where
        T: Send + for<'r> sqlx::FromRow<'r, SqliteRow> + Unpin,
    {
        sqlx::query_as_with(self.query.sql(), self.arguments)
            .fetch_one(self.pool.as_ref())
            .await
    }

    pub async fn fetch_all<T>(self) -> Result<Vec<T>, sqlx::Error>
    where
        T: Send + for<'r> sqlx::FromRow<'r, SqliteRow> + Unpin,
    {
        sqlx::query_as_with(self.query.sql(), self.arguments)
            .fetch_all(self.pool.as_ref())
            .await
    }

    pub async fn fetch_optional<T>(self) -> Result<Option<T>, sqlx::Error>
    where
        T: Send + for<'r> sqlx::FromRow<'r, SqliteRow> + Unpin,
    {
        sqlx::query_as_with(self.query.sql(), self.arguments)
            .fetch_optional(self.pool.as_ref())
            .await
    }
}
