#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![deny(clippy::pedantic)]
#![deny(clippy::get_unwrap)]
#![allow(clippy::module_name_repetitions)]

mod config;
mod context;
mod create_secret;
mod domain;
mod error;
mod hard_delete_secret;
mod soft_delete_secret;
mod store;

use std::sync::Arc;

use context::Context;
use covert_storage::{
    migrator::{migration_scripts, MigrationError},
    BackendStoragePool,
};
use rust_embed::RustEmbed;

use self::{
    config::{read_config, set_config},
    create_secret::{add_secret, read_secret},
    hard_delete_secret::hard_delete_secret,
    soft_delete_secret::{path_undelete_write, soft_delete_secret},
};
use covert_framework::{create, extract::Extension, read, Backend, Router};
use covert_types::backend::{BackendCategory, BackendType};

#[derive(RustEmbed)]
#[folder = "migrations/"]
struct Migrations;

/// Returns a new versioned KV secret engine.
///
/// # Errors
///
/// Returns an error if it fails to read the migration scripts.
pub fn new_versioned_kv_backend(storage: BackendStoragePool) -> Result<Backend, MigrationError> {
    let ctx = Context::new(storage);

    let router = Router::new()
        .route(
            "/config",
            read(read_config).update(set_config).create(set_config),
        )
        .route(
            "/data/*path",
            read(read_secret).create(add_secret).update(add_secret),
        )
        .route(
            "/delete/*path",
            create(soft_delete_secret).update(soft_delete_secret),
        )
        .route(
            "/undelete/*path",
            create(path_undelete_write).update(path_undelete_write),
        )
        .route(
            "/destroy/*path",
            create(hard_delete_secret).update(hard_delete_secret),
        )
        .layer(Extension(Arc::new(ctx)))
        .build()
        .into_service();

    let migrations = migration_scripts::<Migrations>()?;

    Ok(Backend {
        handler: router,
        category: BackendCategory::Logical,
        variant: BackendType::Kv,
        migrations,
    })
}
