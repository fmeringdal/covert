use covert_storage::{migrator::MigrationError, EncryptedPool};
use rust_embed::RustEmbed;
use sqlx::{Pool, Sqlite};

use crate::error::Error;

#[derive(RustEmbed)]
#[folder = "migrations/encrypted/"]
pub(crate) struct Migrations;

#[derive(RustEmbed)]
#[folder = "migrations/unecrypted/"]
pub(crate) struct UnecryptedDbMigrations;

// TODO: this needs to be improved to keep track of migrations metadata
async fn migrate<'e, 'c, M, E>(executor: E) -> Result<(), Error>
where
    M: RustEmbed,
    'c: 'e,
    E: sqlx::Executor<'c, Database = Sqlite> + Copy,
{
    let migrations = covert_storage::migrator::migration_scripts::<M>()?;

    for migration in migrations {
        sqlx::query(&migration.script)
            .execute(executor)
            .await
            .map_err(|error| MigrationError::Execution {
                filename: migration.description,
                error,
            })?;
    }
    Ok(())
}

pub(crate) async fn migrate_unecrypted_db(pool: &Pool<Sqlite>) -> Result<(), Error> {
    migrate::<UnecryptedDbMigrations, &Pool<Sqlite>>(pool).await
}

pub(crate) async fn migrate_ecrypted_db(pool: &EncryptedPool) -> Result<(), Error> {
    migrate::<Migrations, &EncryptedPool>(pool).await
}
