use std::pin::Pin;
use std::{future::Future, sync::Arc};

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

use crate::{
    error::{Error, ErrorType},
    repos::{namespace::Namespace, Repos},
    ExpirationManager, Router,
};

use super::mount::remove_mount;

#[tracing::instrument(skip(repos))]
pub async fn create_namespace_handler(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Json(params): Json<CreateNamespaceParams>,
) -> Result<Response, Error> {
    let new_namespace = Namespace {
        id: Uuid::new_v4().to_string(),
        name: params.name,
        parent_namespace_id: Some(ns.id),
    };
    repos.namespace.create(&new_namespace).await?;

    let resp = CreateNamespaceResponse {
        id: new_namespace.id,
        name: new_namespace.name,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip(repos))]
pub async fn list_namespaces_handler(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
) -> Result<Response, Error> {
    let namespaces = repos.namespace.list(&ns.id).await?;

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

#[tracing::instrument(skip(repos, expiration_manager, router))]
pub async fn delete_namespace_handler(
    Extension(repos): Extension<Repos>,
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(router): Extension<Arc<Router>>,
    Extension(ns): Extension<Namespace>,
    Path(name): Path<String>,
) -> Result<Response, Error> {
    let namespaces = repos.namespace.list(&ns.id).await?;
    let ns_to_delete = namespaces
        .into_iter()
        .find(|ns| ns.name == name)
        .ok_or_else(|| ErrorType::NotFound(format!("Namespace `{name}` not found")))?;

    delete_namespace(
        repos.clone(),
        Arc::clone(&router),
        Arc::clone(&expiration_manager),
        ns_to_delete.id.clone(),
    )
    .await?;

    let resp = DeleteNamespaceResponse {
        id: ns_to_delete.id,
        name: ns_to_delete.name,
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub fn delete_namespace(
    repos: Repos,
    router: Arc<Router>,
    expiration_manager: Arc<ExpirationManager>,
    namespace_id: String,
) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send>> {
    Box::pin(async move {
        // Remove all child namespaces
        let child_namespaces = repos.namespace.list(&namespace_id).await?;
        for child_ns in child_namespaces {
            delete_namespace(
                repos.clone(),
                Arc::clone(&router),
                Arc::clone(&expiration_manager),
                child_ns.id,
            )
            .await?;
        }

        // Remove all mounts from namespace
        let mounts = repos.mount.list(&namespace_id).await?;
        for mount in mounts {
            remove_mount(
                &repos,
                &router,
                &expiration_manager,
                &mount.path,
                &namespace_id,
            )
            .await?;
        }

        if !repos.namespace.delete(&namespace_id).await? {
            return Err(ErrorType::InternalError(anyhow::Error::msg(format!(
                "Failed to remove namespace with id `{namespace_id}`"
            ))))?;
        }
        Ok(())
    })
}
