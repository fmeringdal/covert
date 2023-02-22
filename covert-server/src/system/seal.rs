use std::sync::Arc;

use covert_framework::extract::Extension;
use covert_types::{methods::system::SealResponse, response::Response};
use tracing::info;

use crate::{
    error::{Error, ErrorType},
    repos::{namespace::Namespace, Repos},
    ExpirationManager, Router,
};

pub async fn handle_seal(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(router): Extension<Arc<Router>>,
) -> Result<Response, Error> {
    if ns.parent_namespace_id.is_some() {
        return Err(ErrorType::SealInNonRootNamespace.into());
    }
    seal(&repos, expiration_manager, router).await?;

    let resp = SealResponse {
        message: "Successfully sealed".into(),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip_all)]
async fn seal(
    repos: &Repos,
    expiration_manager: Arc<ExpirationManager>,
    router: Arc<Router>,
) -> Result<(), Error> {
    info!("Sealing the storage");
    repos.pool.seal()?;

    // Stop expiration manager
    expiration_manager.stop().await;

    // Clear all the route entries except system
    let system = router.get_system_mount().await.ok_or_else(|| {
        ErrorType::InternalError(anyhow::Error::msg(
            "router does not contain the system backend",
        ))
    })?;
    router.clear_mounts().await;
    router.mount_system(system).await;

    Ok(())
}
