#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![deny(clippy::pedantic)]
#![deny(clippy::get_unwrap)]
#![allow(clippy::module_name_repetitions)]

mod backend_pool;
mod encrypted_pool;
pub mod migrator;
mod scoped_queries;
mod states;
mod storage;
mod utils;

pub use backend_pool::BackendStoragePool;
pub use encrypted_pool::{EncryptedPool, EncryptedPoolError, PoolState};
