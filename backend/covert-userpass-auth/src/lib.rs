#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![deny(clippy::pedantic)]
#![deny(clippy::get_unwrap)]
#![allow(clippy::module_name_repetitions)]

mod error;
mod store;

use std::sync::Arc;

use bcrypt::{hash, verify};
use covert_framework::{
    create, delete,
    extract::{Extension, Json, Path},
    update, update_with_config, Backend, RouteConfig, Router,
};
use covert_storage::{
    migrator::{migration_scripts, MigrationError},
    BackendStoragePool,
};
use covert_types::{
    backend::{BackendCategory, BackendType},
    methods::userpass::{
        CreateUserParams, CreateUserResponse, ListUsersResponse, LoginParams, RemoveUserResponse,
        UpdateUserPasswordParams, UpdateUserPasswordResponse, UserListItem,
    },
    response::Response,
};
use covert_types::{mount::MountConfig, response::AuthResponse};
use error::{Error, ErrorType};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use store::user::UsersRepo;

// TODO: maybe increase this
const DEFAULT_COST: u32 = 8;

pub struct Context {
    users_repo: UsersRepo,
}

#[derive(RustEmbed)]
#[folder = "migrations/"]
struct Migrations;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq, Clone)]
pub struct User {
    username: String,
    password: String,
}

/// Returns a new userpass auth method.
///
/// # Errors
///
/// Returns an error if it fails to read the migration scripts.
pub fn new_userpass_backend(pool: BackendStoragePool) -> Result<Backend, MigrationError> {
    let ctx = Context {
        users_repo: UsersRepo::new(pool),
    };

    let router = Router::new()
        .route(
            "/login",
            update_with_config(login, RouteConfig::unauthenticated())
                .create_with_config(login, RouteConfig::unauthenticated()),
        )
        .route("/users", create(create_user).read(list_users))
        .route("/users/:username", delete(remove_user))
        .route("/users/:username/password", update(update_user_password))
        .layer(Extension(Arc::new(ctx)))
        .build()
        .into_service();

    let migrations = migration_scripts::<Migrations>()?;

    Ok(Backend {
        handler: router,
        category: BackendCategory::Credential,
        variant: BackendType::Userpass,
        migrations,
    })
}

#[tracing::instrument(skip_all)]
async fn user_by_username_and_password(
    ctx: &Context,
    username: &str,
    password: &str,
) -> Result<User, Error> {
    let user = ctx
        .users_repo
        .get(username)
        .await?
        .ok_or_else(|| ErrorType::UserNotFound {
            username: username.to_string(),
        })?;

    if !matches!(verify(password, &user.password), Ok(true)) {
        return Err(ErrorType::IncorrectPassword.into());
    }

    Ok(user)
}

#[tracing::instrument(skip_all, fields(username = params.username))]
async fn login(
    Json(params): Json<LoginParams>,
    Extension(config): Extension<MountConfig>,
    Extension(ctx): Extension<Arc<Context>>,
) -> Result<Response, Error> {
    let _user = user_by_username_and_password(&ctx, &params.username, &params.password).await?;

    let auth = AuthResponse {
        alias: params.username,
        ttl: Some(config.default_lease_ttl),
    };
    Ok(Response::Auth(auth))
}

#[tracing::instrument(skip_all, fields(username = params.username))]
async fn create_user(
    Json(params): Json<CreateUserParams>,
    Extension(ctx): Extension<Arc<Context>>,
) -> Result<Response, Error> {
    let password =
        hash(&params.password, DEFAULT_COST).map_err(|_| ErrorType::UnsupportedPassword)?;
    let user = User {
        username: params.username,
        password,
    };
    ctx.users_repo.create(&user).await?;

    let resp = CreateUserResponse {
        username: user.username,
    };
    Response::raw(resp).map_err(Into::into)
}

#[tracing::instrument(skip_all)]
async fn list_users(Extension(ctx): Extension<Arc<Context>>) -> Result<Response, Error> {
    let users = ctx.users_repo.list().await?;

    let resp = ListUsersResponse {
        users: users
            .into_iter()
            .map(|user| UserListItem {
                username: user.username,
            })
            .collect(),
    };
    Response::raw(resp).map_err(Into::into)
}

#[tracing::instrument(skip_all, fields(username = username))]
async fn update_user_password(
    Json(params): Json<UpdateUserPasswordParams>,
    Path(username): Path<String>,
    Extension(ctx): Extension<Arc<Context>>,
) -> Result<Response, Error> {
    let _user = user_by_username_and_password(&ctx, &username, &params.password).await?;
    let new_password =
        hash(&params.new_password, DEFAULT_COST).map_err(|_| ErrorType::UnsupportedPassword)?;
    ctx.users_repo
        .update_password(&username, &new_password)
        .await?;

    let resp = UpdateUserPasswordResponse { username };
    Response::raw(resp).map_err(Into::into)
}

#[tracing::instrument(skip_all, fields(username = username))]
async fn remove_user(
    Path(username): Path<String>,
    Extension(ctx): Extension<Arc<Context>>,
) -> Result<Response, Error> {
    ctx.users_repo.remove(&username).await?;

    let resp = RemoveUserResponse { username };
    Response::raw(resp).map_err(Into::into)
}
