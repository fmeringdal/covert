use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use crate::{
    error::{Error, ErrorType},
    path_roles::RoleEntry,
};

use super::Context;

use covert_framework::extract::{Extension, Json, Path};
use covert_types::{
    methods::psql::CreateRoleCredsParams,
    mount::MountConfig,
    psql::RoleCredentials,
    response::{LeaseRenewRevokeEndpoint, LeaseResponse, Response},
    ttl::calculate_ttl,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct RoleInfo {
    pub username: String,
    pub role: String,
}

#[tracing::instrument(skip(b), fields(role_name = name))]
pub async fn generate_role_credentials(
    Extension(b): Extension<Arc<Context>>,
    Extension(config): Extension<MountConfig>,
    Json(params): Json<CreateRoleCredsParams>,
    Path(name): Path<String>,
) -> Result<Response, Error> {
    let role = b
        .role_repo
        .get(&name)
        .await?
        .ok_or_else(|| ErrorType::RoleNotFound { name: name.clone() })?;

    // Generate the username, password and expiration.
    let username_suffix = Uuid::new_v4().to_string();
    let mut username = format!("{name}-{username_suffix}");
    // PG limits user to 63 characters
    if username.len() > 63 {
        username = username[..=63].to_string();
    }
    let password = Uuid::new_v4().to_string();

    let now = Utc::now();
    let issued_at = now;
    let ttl = calculate_ttl(now, issued_at, &config, params.ttl)
        .map_err(|_| ErrorType::InternalError(anyhow::Error::msg("Unable to calculate TTL")))?;

    let expiration = now + ttl;

    let expiration = expiration.format("%Y-%m-%d %H:%M:%S").to_string();

    // Get our handle
    let pool = b.pool().await?;
    create_psql_role(&pool, &role, &username, &password, &expiration).await?;

    // Return the secret
    let role_info = RoleInfo {
        username: username.clone(),
        role: name.clone(),
    };
    let creds = RoleCredentials { username, password };
    let lease =
        LeaseResponse {
            renew: LeaseRenewRevokeEndpoint {
                path: "creds".into(),
                data: serde_json::to_value(&role_info)?,
            },
            revoke: LeaseRenewRevokeEndpoint {
                path: "creds".into(),
                data: serde_json::to_value(&role_info)?,
            },
            data: serde_json::to_value(&creds)?,
            ttl: Some(ttl.to_std().map_err(|_| {
                ErrorType::InternalError(anyhow::Error::msg("Unable to create TTL"))
            })?),
        };
    Ok(Response::Lease(lease))
}

#[tracing::instrument(skip_all)]
async fn create_psql_role(
    pool: &Pool<Postgres>,
    role: &RoleEntry,
    username: &str,
    password: &str,
    expiration: &str,
) -> Result<(), Error> {
    // Start a transaction
    let mut tx = pool.begin().await?;

    // Execute each query
    let sql = role
        .sql
        .replace("{{name}}", username)
        .replace("{{password}}", password)
        .replace("{{expiration}}", expiration);
    for query in sql.split(';') {
        sqlx::query(query).execute(&mut tx).await?;
    }

    // Commit the transaction
    tx.commit().await?;

    Ok(())
}
