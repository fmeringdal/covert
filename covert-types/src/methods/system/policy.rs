use serde::{Deserialize, Serialize};

use crate::policy::Policy;

#[derive(Debug, Deserialize, Serialize)]
pub struct CreatePolicyParams {
    pub name: String,
    pub policy: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreatePolicyResponse {
    pub policy: Policy,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListPolicyResponse {
    pub policies: Vec<Policy>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemovePolicyResponse {
    pub policy: String,
}
