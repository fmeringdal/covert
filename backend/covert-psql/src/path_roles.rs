use std::sync::Arc;

use crate::error::Error;

use super::Context;

use covert_framework::extract::{Extension, Json, Path};
use covert_types::{
    methods::psql::{CreateRoleParams, CreateRoleResponse},
    response::Response,
};

#[derive(Debug, Clone, sqlx::FromRow, PartialEq, Eq)]
pub struct RoleEntry {
    pub sql: String,
    pub revocation_sql: String,
}

#[tracing::instrument(skip_all, fields(role_name = name, role = ?body))]
pub async fn path_role_create(
    Extension(b): Extension<Arc<Context>>,
    Path(name): Path<String>,
    Json(body): Json<CreateRoleParams>,
) -> Result<Response, Error> {
    // TODO: validate sql and revocation sql statements

    // Store it
    let role = RoleEntry {
        sql: body.sql,
        revocation_sql: body.revocation_sql,
    };
    b.role_repo.create(&name, &role).await?;

    let resp = CreateRoleResponse {
        sql: role.sql,
        revocation_sql: role.revocation_sql,
    };
    Response::raw(resp).map_err(Into::into)
}
