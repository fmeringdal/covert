use sqlx::{Pool, Sqlite};
use tempfile::TempDir;

#[derive(Debug)]
pub struct Uninitialized;

#[derive(Debug)]
pub struct Sealed;

#[derive(Debug)]
pub struct Unsealed {
    pub pool: Pool<Sqlite>,
    pub tmpdir: Option<TempDir>,
}
