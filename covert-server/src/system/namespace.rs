use std::future::Future;
use std::pin::Pin;

use covert_framework::extract::{Extension, Json, Path};
use covert_types::methods::system::DeleteNamespaceResponse;
use covert_types::{
    methods::system::{
        CreateNamespaceParams, CreateNamespaceResponse, ListNamespaceItemResponse,
        ListNamespaceResponse,
    },
    response::Response,
};
use uuid::Uuid;

use crate::context::Context;
use crate::{
    error::{Error, ErrorType},
    repos::namespace::Namespace,
};

use super::mount::remove_mount;

#[tracing::instrument(skip(ctx))]
pub async fn create_namespace_handler(
    Extension(ctx): Extension<Context>,
    Extension(ns): Extension<Namespace>,
    Json(params): Json<CreateNamespaceParams>,
) -> Result<Response, Error> {
    let new_namespace = Namespace {
        id: Uuid::new_v4().to_string(),
        name: params.name,
        parent_namespace_id: Some(ns.id),
    };
    ctx.repos.namespace.create(&new_namespace).await?;

    let resp = CreateNamespaceResponse {
        id: new_namespace.id,
        name: new_namespace.name,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(ctx))]
pub async fn list_namespaces_handler(
    Extension(ctx): Extension<Context>,
    Extension(ns): Extension<Namespace>,
) -> Result<Response, Error> {
    let namespaces = ctx.repos.namespace.list(&ns.id).await?;

    let resp = ListNamespaceResponse {
        namespaces: namespaces
            .into_iter()
            .map(|ns| ListNamespaceItemResponse {
                id: ns.id,
                name: ns.name,
            })
            .collect(),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(ctx))]
pub async fn delete_namespace_handler(
    Extension(ctx): Extension<Context>,
    Extension(ns): Extension<Namespace>,
    Path(name): Path<String>,
) -> Result<Response, Error> {
    let namespaces = ctx.repos.namespace.list(&ns.id).await?;
    let ns_to_delete = namespaces
        .into_iter()
        .find(|ns| ns.name == name)
        .ok_or_else(|| ErrorType::NotFound(format!("Namespace `{name}` not found")))?;

    delete_namespace(ctx, ns_to_delete.id.clone()).await?;

    let resp = DeleteNamespaceResponse {
        id: ns_to_delete.id,
        name: ns_to_delete.name,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub fn delete_namespace(
    ctx: Context,
    namespace_id: String,
) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send>> {
    Box::pin(async move {
        // Remove all child namespaces
        let child_namespaces = ctx.repos.namespace.list(&namespace_id).await?;
        for child_ns in child_namespaces {
            delete_namespace(ctx.clone(), child_ns.id).await?;
        }

        // Remove all mounts from namespace
        let mounts = ctx.repos.mount.list(&namespace_id).await?;
        for mount in mounts {
            remove_mount(&ctx, &mount.path, &namespace_id).await?;
        }

        if !ctx.repos.namespace.delete(&namespace_id).await? {
            return Err(ErrorType::InternalError(anyhow::Error::msg(format!(
                "Failed to remove namespace with id `{namespace_id}`"
            ))))?;
        }
        Ok(())
    })
}
