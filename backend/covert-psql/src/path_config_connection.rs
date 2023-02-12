use std::sync::Arc;

use sqlx::Connection;

use crate::{
    error::{Error, ErrorType},
    pool_from_config,
};

use super::Context;
use covert_framework::extract::{Extension, Json};
use covert_types::{
    methods::psql::{ReadConnectionResponse, SetConnectionParams, SetConnectionResponse},
    psql::ConnectionConfig,
    response::Response,
};

#[tracing::instrument(skip_all)]
pub async fn path_connection_write(
    Extension(b): Extension<Arc<Context>>,
    Json(body): Json<SetConnectionParams>,
) -> Result<Response, Error> {
    let connection_url = body.connection_url;
    let max_open_connections = body.max_open_connections.unwrap_or(2);

    let connection_config = ConnectionConfig {
        connection_url,
        max_open_connections,
    };

    // Don't check the connection_url if verification is disabled
    if body.verify_connection {
        let pool = pool_from_config(&connection_config).await?;

        let mut connection = pool.acquire().await?;
        if connection.ping().await.is_err() {
            return Err(ErrorType::InvalidConnectionString.into());
        }
        tracing::info!("Successfully verified connection string to database");
    }

    // Store it
    b.connection_repo.set(&connection_config).await?;

    // Reset the DB connection
    b.set_pool().await?;

    // resp := &logical.Response{}
    // resp.AddWarning("Read access to this endpoint should be controlled via ACLs as it will return the connection string or URL as it is, including passwords, if any.")
    let resp = SetConnectionResponse {
        connection: connection_config,
    };
    Response::raw(resp).map_err(Into::into)
}

#[tracing::instrument(skip_all)]
pub async fn path_connection_read(
    Extension(b): Extension<Arc<Context>>,
) -> Result<Response, Error> {
    let connection_config = b.connection_repo.get().await?;

    let resp = ReadConnectionResponse {
        connection: connection_config,
    };
    Response::raw(resp).map_err(Into::into)
}
