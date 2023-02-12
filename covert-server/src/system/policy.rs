use std::sync::Arc;

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
    store::policy_store::PolicyStore,
};

pub async fn handle_create_policy(
    Extension(policy_store): Extension<Arc<PolicyStore>>,
    Json(body): Json<CreatePolicyParams>,
) -> Result<Response, Error> {
    let path_policies = PathPolicy::parse(&body.policy)
        .map_err(|_| ErrorType::BadRequest("Malformed policy".into()))?;
    let policy = Policy::new(body.name, path_policies);
    policy_store.create(&policy).await?;
    let resp = CreatePolicyResponse { policy };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_list_policies(
    Extension(policy_store): Extension<Arc<PolicyStore>>,
) -> Result<Response, Error> {
    let policies = policy_store.list().await?;
    let resp = ListPolicyResponse { policies };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}

pub async fn handle_delete_policy(
    Extension(policy_store): Extension<Arc<PolicyStore>>,
    Path(name): Path<String>,
) -> Result<Response, Error> {
    if !policy_store.remove(&name).await? {
        return Err(ErrorType::NotFound(format!("Policy `{name}` not found")).into());
    }
    let resp = RemovePolicyResponse { policy: name };
    Response::raw(resp).map_err(|err| ErrorType::BadResponseData(err).into())
}
