use covert_framework::extract::Extension;
use covert_types::{methods::system::SealResponse, response::Response};
use tracing::info;

use crate::{
    context::Context,
    error::{Error, ErrorType},
    repos::namespace::Namespace,
};

pub async fn handle_seal(
    Extension(ctx): Extension<Context>,
    Extension(ns): Extension<Namespace>,
) -> Result<Response, Error> {
    if ns.parent_namespace_id.is_some() {
        return Err(ErrorType::SealInNonRootNamespace.into());
    }
    seal(&ctx).await?;

    let resp = SealResponse {
        message: "Successfully sealed".into(),
    };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

#[tracing::instrument(skip_all)]
async fn seal(ctx: &Context) -> Result<(), Error> {
    info!("Sealing the storage");
    ctx.repos.pool.seal()?;

    // Stop expiration manager
    ctx.expiration_manager.stop().await;

    // Clear all the route entries except system
    let system = ctx.router.get_system_mount().ok_or_else(|| {
        ErrorType::InternalError(anyhow::Error::msg(
            "router does not contain the system backend",
        ))
    })?;
    ctx.router.clear_mounts();
    ctx.router.mount_system(system);

    Ok(())
}
