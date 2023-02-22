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
    repos::{namespace::Namespace, Repos},
};

pub async fn handle_create_policy(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Json(body): Json<CreatePolicyParams>,
) -> Result<Response, Error> {
    let path_policies = PathPolicy::parse(&body.policy)
        .map_err(|_| ErrorType::BadRequest("Malformed policy".into()))?;
    let policy = Policy::new(body.name, path_policies, ns.id.clone());
    repos.policy.create(&policy).await?;
    let resp = CreatePolicyResponse { policy };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_list_policies(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
) -> Result<Response, Error> {
    let policies = repos.policy.list(&ns.id).await?;
    let resp = ListPolicyResponse { policies };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_delete_policy(
    Extension(repos): Extension<Repos>,
    Extension(ns): Extension<Namespace>,
    Path(name): Path<String>,
) -> Result<Response, Error> {
    if !repos.policy.remove(&name, &ns.id).await? {
        return Err(ErrorType::NotFound(format!("Policy `{name}` not found")).into());
    }
    let resp = RemovePolicyResponse { policy: name };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
