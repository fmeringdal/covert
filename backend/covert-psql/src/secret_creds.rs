use std::sync::Arc;

use crate::error::{Error, ErrorType};

use super::{path_role_create::RoleInfo, Context};
use chrono::Utc;
use covert_framework::extract::{Extension, Json};
use covert_types::{methods::psql::RenewLeaseResponse, mount::MountConfig, response::Response};
use tracing::debug;

#[tracing::instrument(skip(b))]
pub async fn secret_creds_revoke(
    Extension(b): Extension<Arc<Context>>,
    Json(body): Json<RoleInfo>,
) -> Result<Response, Error> {
    debug!("revoking creds");
    let role = b
        .role_repo
        .get(&body.role)
        .await?
        .ok_or_else(|| ErrorType::RoleNotFound {
            name: body.role.clone(),
        })?;

    // Get our connection
    let pool = b.pool().await?;

    let revocation_sql = role.revocation_sql.replace("{{name}}", &body.username);
    // Start a transaction
    let mut tx = pool.begin().await?;
    for query in revocation_sql.split(';') {
        sqlx::query(query).execute(&mut tx).await?;
    }
    // Commit the transaction
    tx.commit().await?;

    Ok(Response::ok())
}

// TODO: this is never used yet
#[tracing::instrument(skip(b))]
pub async fn secret_creds_renew(
    Extension(b): Extension<Arc<Context>>,
    Extension(config): Extension<MountConfig>,
    Json(body): Json<RoleInfo>,
) -> Result<Response, Error> {
    debug!("renewing creds");
    b.role_repo
        .get(&body.role)
        .await?
        .ok_or_else(|| ErrorType::RoleNotFound {
            name: body.role.clone(),
        })?;

    let std_ttl = config.default_lease_ttl;
    let ttl = chrono::Duration::from_std(std_ttl)
        .map_err(|_| ErrorType::InternalError(anyhow::Error::msg("Unable to create TTL")))?;
    let expiration = Utc::now() + ttl;
    // TODO: correct format
    // 	Format("2006-01-02 15:04:05-0700")
    let expiration = expiration.format("%Y-%m-%d %H:%M:%S").to_string();

    // Get our connection
    let pool = b.pool().await?;

    // TODO: move role to argument to avoid sql injection
    sqlx::query(&format!(
        "ALTER ROLE \"{}\" VALID UNTIL '{expiration}'",
        body.username
    ))
    .execute(&*pool)
    .await?;

    let resp = RenewLeaseResponse { ttl: std_ttl };
    Response::raw(resp).map_err(Into::into)
}
