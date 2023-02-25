use covert_storage::{migrator::MigrationError, EncryptedPool};
use sqlx::{Pool, Sqlite};

use crate::error::{Error, ErrorType};

pub(crate) async fn migrate_unecrypted_db(pool: &Pool<Sqlite>) -> Result<(), Error> {
    sqlx::migrate!("migrations/unecrypted")
        .run(pool)
        .await
        .map_err(|err| ErrorType::Migration(MigrationError::DB(err.into())).into())
}

pub(crate) async fn migrate_ecrypted_db(pool: &EncryptedPool) -> Result<(), Error> {
    sqlx::migrate!("migrations/encrypted")
        .run(pool)
        .await
        .map_err(|err| ErrorType::Migration(MigrationError::DB(err.into())).into())
}
