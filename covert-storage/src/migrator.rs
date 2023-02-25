use sha2::{Digest, Sha384};
use sqlx::Executor;

use crate::scoped_queries::ScopedQuery;
use crate::{BackendStoragePool, EncryptedPool};

const BACKEND_MIGRATIONS_TABLE: &str = "_BACKEND_STORAGE_MIGRATIONS";

#[derive(Debug)]
pub struct MigrationScript {
    pub script: String,
    pub description: String,
}

async fn create_migrate_table(pool: &EncryptedPool) -> Result<(), sqlx::Error> {
    // TODO: is mount_id here the correct type?
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {BACKEND_MIGRATIONS_TABLE}(
        mount_id INTEGER NOT NULL REFERENCES MOUNTS(id) ON DELETE CASCADE ON UPDATE CASCADE,
        version INTEGER NOT NULL,
        description TEXT NOT NULL,
        checksum BLOB NOT NULL,
        created_at TIMESTAMP NOT NULL,
        PRIMARY KEY(mount_id, version)
    )"
    );
    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct LatestMigration {
    latest_version: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("sqlx error")]
    DB(sqlx::Error),
    #[error("bad query")]
    BadQuery,
    #[error("unable to parse migration script `{filename}`")]
    Script { filename: String, error: String },
    #[error("unable to execute migration script `{filename}`")]
    Execution {
        filename: String,
        error: sqlx::Error,
    },
}

impl From<sqlx::Error> for MigrationError {
    fn from(err: sqlx::Error) -> Self {
        Self::DB(err)
    }
}

/// Apply [`MigrationScript`]'s for a backend by applying the mount id and prefix.
///
/// # Errors
///
/// Returns error if it fails to apply any of the migration scripts.
pub async fn migrate(
    pool: &EncryptedPool,
    migrations: &[MigrationScript],
    mount_id: &str,
    prefix: &str,
) -> Result<(), MigrationError> {
    create_migrate_table(pool).await?;

    let latest_migration: Option<LatestMigration> = sqlx::query_as(&format!(
        "SELECT MAX(version) AS latest_version FROM {BACKEND_MIGRATIONS_TABLE} 
        WHERE mount_id = ?"
    ))
    .bind(mount_id)
    .fetch_optional(pool)
    .await?;
    let last_migration_version = latest_migration.and_then(|m| m.latest_version);

    for (version, migration) in migrations.iter().enumerate() {
        if let Some(last_migration_version) = last_migration_version {
            if last_migration_version >= version as i64 {
                continue;
            }
        }
        // First check if scoped query is valid
        let sql =
            ScopedQuery::new(prefix, &migration.script).map_err(|_| MigrationError::BadQuery)?;
        let checksum = Sha384::digest(sql.sql().as_bytes()).to_vec();

        let mut tx = pool.begin().await?;

        // Try to add new migration version for backend
        sqlx::query(&format!(
            "INSERT INTO {BACKEND_MIGRATIONS_TABLE} (
        mount_id,
        version,
        description,
        checksum,
        created_at
    ) VALUES (
        ?,
        ?,
        ?,
        ?,
        ?
    )"
        ))
        .bind(mount_id)
        .bind(version as i64)
        .bind(&migration.description)
        .bind(checksum)
        .bind(chrono::Utc::now())
        .execute(&mut tx)
        .await
        .map_err(|_| MigrationError::BadQuery)?;

        // Migration script
        tx.execute(sql.sql()).await?;

        tx.commit().await?;
    }

    Ok(())
}

/// Run migrations for a given backend.
///
/// # Errors
///
/// Returns error if the migration fails to read the migration file contents
/// or fails to apply any of the migrations.
pub async fn migrate_backend<M: rust_embed::RustEmbed>(
    storage: &BackendStoragePool,
) -> Result<(), MigrationError> {
    let migrations = migration_scripts::<M>()?;

    for migration in migrations {
        storage
            .query(&migration.script)
            .map_err(|error| MigrationError::Execution {
                filename: migration.description.clone(),
                error,
            })?
            .execute()
            .await
            .map_err(|error| MigrationError::Execution {
                filename: migration.description,
                error,
            })?;
    }
    Ok(())
}

/// Retrieve [`MigrationScript`]'s from type that implements [`rust_embed::RustEmbed`].
///
/// # Errors
///
/// Returns error if it is unable to parse the contents of any of the migration
/// script files.
pub fn migration_scripts<M: rust_embed::RustEmbed>() -> Result<Vec<MigrationScript>, MigrationError>
{
    let mut migrations = M::iter().collect::<Vec<_>>();
    migrations.sort();

    let mut migration_scripts = vec![];
    for migration_file_name in migrations {
        if let Some(migration) = M::get(&migration_file_name) {
            let sql =
                String::from_utf8(migration.data.to_vec()).map_err(|_| MigrationError::Script {
                    error: "Unable to parse migration script to UTF-8".to_string(),
                    filename: migration_file_name.to_string(),
                })?;
            migration_scripts.push(MigrationScript {
                description: migration_file_name.to_string(),
                script: sql,
            });
        } else {
            return Err(MigrationError::Script {
                filename: migration_file_name.to_string(),
                error: "Unable to get migration script".to_string(),
            });
        }
    }

    Ok(migration_scripts)
}

#[derive(Debug, sqlx::FromRow)]
pub struct Migration {
    pub mount_id: i64,
    pub version: i64,
    pub description: String,
    pub checksum: Vec<u8>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List applied migrations
///
/// # Errors
///
/// Returns error if query to retrieve migrations fails.
pub async fn list_migrations(
    pool: &EncryptedPool,
    mount_id: &str,
) -> Result<Vec<Migration>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT * FROM {BACKEND_MIGRATIONS_TABLE} WHERE mount_id = ?"
    ))
    .bind(mount_id)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, sqlx::FromRow, PartialEq, Eq)]
    struct Tables {
        name: String,
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn migration_works() {
        let pool = EncryptedPool::new(&":memory:".to_string());
        let master_key = pool.initialize().unwrap().unwrap();
        pool.unseal(master_key).unwrap();

        let mount_id = "12421412";
        let prefix = "foo_bar_";

        // create dummy mounts table
        sqlx::query("CREATE TABLE MOUNTS ( id INTEGER PRIMARY KEY )")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO MOUNTS (id) VALUES (?)")
            .bind(mount_id)
            .execute(&pool)
            .await
            .unwrap();

        let mut migrations = vec![
            MigrationScript {
                description: "2022-12-12-init.sql".into(),
                script: r#"
CREATE TABLE IF NOT EXISTS SECRETS (
    "key" TEXT NOT NULL,
    "version" INTEGER NOT NULL, 
    "value" TEXT,
    created_time TIMESTAMP NOT NULL,
    deleted BOOLEAN NOT NULL DEFAULT FALSE,
    destroyed BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY("key", "version"),
    CONSTRAINT destroyed_secret CHECK (
        -- If not destroyed then value is *not* null
        (NOT(destroyed) AND "value" IS NOT NULL) OR
        -- If destroyed then value is null
        (destroyed AND "value" IS NULL) 
    )
); 


CREATE TABLE IF NOT EXISTS CONFIG (
    lock INTEGER PRIMARY KEY DEFAULT 1,
    max_versions INTEGER NOT NULL DEFAULT 10,

    -- Used to ensure that maximum one config is ever inserted
    CONSTRAINT CONFIG_LOCK CHECK (lock=1)
); 
                
                "#
                .to_string(),
            },
            MigrationScript {
                description: "2022-12-14-add-user.sql".into(),
                script: r#"
CREATE TABLE IF NOT EXISTS USERS (
    uid INTEGER PRIMARY KEY,
    "name" TEXT NOT NULL
); 
                "#
                .to_string(),
            },
        ];
        migrate(&pool, &migrations, mount_id, prefix).await.unwrap();

        let res: Vec<Migration> =
            sqlx::query_as(&format!("SELECT * FROM {BACKEND_MIGRATIONS_TABLE}"))
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(res.len(), 2);

        migrations.push(MigrationScript {
            description: "2022-12-16-add-user-email.sql".into(),
            script: r#"
ALTER TABLE USERS 
    ADD email TEXT; 
            "#
            .to_string(),
        });
        migrate(&pool, &migrations, mount_id, prefix).await.unwrap();

        let res: Vec<Migration> =
            sqlx::query_as(&format!("SELECT * FROM {BACKEND_MIGRATIONS_TABLE}"))
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(res.len(), 3);

        // Run again and nothing should change
        migrate(&pool, &migrations, mount_id, prefix).await.unwrap();
        let res: Vec<Migration> = list_migrations(&pool, mount_id).await.unwrap();
        assert_eq!(res.len(), 3);

        // List tables
        let res: Vec<Tables> = sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table'")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(
            res,
            vec![
                Tables {
                    name: "MOUNTS".to_string()
                },
                Tables {
                    name: BACKEND_MIGRATIONS_TABLE.to_string()
                },
                // Tables from migrations
                Tables {
                    name: format!("{prefix}SECRETS")
                },
                Tables {
                    name: format!("{prefix}CONFIG")
                },
                Tables {
                    name: format!("{prefix}USERS")
                }
            ]
        );

        // Use new mount and it should work again
        let mount_id = "6789";
        let prefix = "foo_foo_";

        let res: Vec<Migration> = list_migrations(&pool, mount_id).await.unwrap();
        assert_eq!(res.len(), 0);

        sqlx::query("INSERT INTO MOUNTS (id) VALUES (?)")
            .bind(mount_id)
            .execute(&pool)
            .await
            .unwrap();
        migrate(&pool, &migrations, mount_id, prefix).await.unwrap();
        let res: Vec<Migration> = list_migrations(&pool, mount_id).await.unwrap();
        assert_eq!(res.len(), 3);
    }
}
