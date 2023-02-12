use sqlx::Sqlite;

use crate::error::Error;

#[derive(Debug, sqlx::FromRow, PartialEq, Eq)]
pub struct Table {
    pub name: String,
}

pub async fn get_resources_by_prefix<'e, 'c, E>(
    executor: E,
    prefix: &str,
) -> Result<Vec<Table>, Error>
where
    E: sqlx::Executor<'c, Database = Sqlite>,
    'c: 'e,
{
    let prefix_pattern = format!("{prefix}%");

    sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE ?")
        .bind(prefix_pattern)
        .fetch_all(executor)
        .await
        .map_err(Into::into)
}

pub async fn drop_table<'e, 'c, E>(executor: E, table_name: &str) -> Result<(), Error>
where
    E: sqlx::Executor<'c, Database = Sqlite>,
    'c: 'e,
{
    // TODO: parameters not working here?
    sqlx::query(&format!("DROP TABLE IF EXISTS {table_name}"))
        .execute(executor)
        .await
        .map_err(Into::into)
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};

    use crate::helpers::sqlite::{drop_table, get_resources_by_prefix, Table};

    #[tokio::test]
    async fn sqlx_cleanup() {
        let opts = SqliteConnectOptions::new()
            .foreign_keys(true)
            .filename(":memory:");
        let mut conn = SqliteConnection::connect_with(&opts).await.unwrap();

        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS FOO_123_users ( id TEXT PRIMARY KEY )"
        ))
        .execute(&mut conn)
        .await
        .unwrap();

        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS FOO_123_books ( id TEXT PRIMARY KEY, author TEXT REFERENCES FOO_123_users(id) )"
        ))
        .execute(&mut conn)
        .await
        .unwrap();

        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS BAR_123_users ( id TEXT PRIMARY KEY )"
        ))
        .execute(&mut conn)
        .await
        .unwrap();

        let tables = get_resources_by_prefix(&mut conn, "foo").await.unwrap();
        assert_eq!(
            tables,
            vec![
                Table {
                    name: "FOO_123_users".into()
                },
                Table {
                    name: "FOO_123_books".into()
                }
            ]
        );

        for table in tables {
            assert!(drop_table(&mut conn, &table.name).await.is_ok());
        }

        let tables = get_resources_by_prefix(&mut conn, "foo").await.unwrap();
        assert_eq!(tables.len(), 0);
    }
}
