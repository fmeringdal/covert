use covert_framework::extract::{Extension, Json, Path};
use covert_types::{
    methods::system::{
        CreatePolicyParams, CreatePolicyResponse, ListPolicyResponse, RemovePolicyResponse,
    },
    policy::{PathPolicy, Policy},
    response::Response,
};

use crate::{
    error::{Error, ErrorType},
    repos::Repos,
};

pub async fn handle_create_policy(
    Extension(repos): Extension<Repos>,
    Json(body): Json<CreatePolicyParams>,
) -> Result<Response, Error> {
    let path_policies = PathPolicy::parse(&body.policy)
        .map_err(|_| ErrorType::BadRequest("Malformed policy".into()))?;
    let policy = Policy::new(body.name, path_policies);
    repos.policy.create(&policy).await?;
    let resp = CreatePolicyResponse { policy };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_list_policies(Extension(repos): Extension<Repos>) -> Result<Response, Error> {
    let policies = repos.policy.list().await?;
    let resp = ListPolicyResponse { policies };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_delete_policy(
    Extension(repos): Extension<Repos>,
    Path(name): Path<String>,
) -> Result<Response, Error> {
    if !repos.policy.remove(&name).await? {
        return Err(ErrorType::NotFound(format!("Policy `{name}` not found")).into());
    }
    let resp = RemovePolicyResponse { policy: name };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
