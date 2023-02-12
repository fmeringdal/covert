use sqlx::{Pool, Sqlite};

#[derive(Debug)]
pub struct Uninitialized;

#[derive(Debug)]
pub struct Sealed;

#[derive(Debug)]
pub struct Unsealed {
    pub pool: Pool<Sqlite>,
}
