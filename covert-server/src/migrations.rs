use covert_storage::{migrator::MigrationError, EncryptedPool};
use rust_embed::RustEmbed;

use crate::error::Error;

#[derive(RustEmbed)]
#[folder = "migrations/"]
pub(crate) struct Migrations;

// TODO: this should be improved
pub(crate) async fn migrate<M: rust_embed::RustEmbed>(pool: &EncryptedPool) -> Result<(), Error> {
    let migrations = covert_storage::migrator::migration_scripts::<M>()?;

    for migration in migrations {
        sqlx::query(&migration.script)
            .execute(pool)
            .await
            .map_err(|error| MigrationError::Execution {
                filename: migration.description,
                error,
            })?;
    }
    Ok(())
}
