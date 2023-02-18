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

pub async fn handle_seal(
    Extension(repos): Extension<Repos>,
    Extension(expiration_manager): Extension<Arc<ExpirationManager>>,
    Extension(router): Extension<Arc<Router>>,
) -> Result<Response, Error> {
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
    info!("Sealing the core");
    repos.pool.seal()?;

    // Stop expiration manager
    expiration_manager.stop().await;

    // Clear all the route entries except system
    router.clear_mounts().await;

    let config = MountConfig::default();
    super::mount::mount(
        repos,
        expiration_manager,
        router,
        SYSTEM_MOUNT_PATH.to_string(),
        BackendType::System,
        config.clone(),
        true,
    )
    .await?;

    Ok(())
}
