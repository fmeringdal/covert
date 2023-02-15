use std::sync::Arc;

use covert_framework::extract::Extension;
use covert_types::{
    backend::BackendType, methods::system::SealResponse, mount::MountConfig, response::Response,
};
use tracing::info;

use crate::{
    error::{Error, ErrorType},
    repos::Repos,
    system::SYSTEM_MOUNT_PATH,
    ExpirationManager, Router,
};

use super::unseal::UnsealProgress;

pub async fn handle_seal(
    Extension(repos): Extension<Repos>,
    Extension(unseal_progress): Extension<UnsealProgress>,
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(router): Extension<Arc<Router>>,
) -> Result<Response, Error> {
    seal(&repos, expiration_manager, router, unseal_progress).await?;

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
    unseal_progress: UnsealProgress,
) -> Result<(), Error> {
    info!("Sealing the core");
    repos.pool.seal()?;

    // Stop expiration manager
    expiration_manager.stop().await;

    // Clear all the route entries except system
    router.clear_mounts().await;

    // This should already be cleared, but just to be safe
    unseal_progress.clear_shares();

    let config = MountConfig::default();
    super::mount::mount(
        repos,
        expiration_manager,
        router,
        unseal_progress,
        SYSTEM_MOUNT_PATH.to_string(),
        BackendType::System,
        config.clone(),
        true,
    )
    .await?;

    Ok(())
}
