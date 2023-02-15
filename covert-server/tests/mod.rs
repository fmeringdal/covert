mod common;

use covert_sdk::{entity::CreateEntityParams, status::CreatePolicyParams};
use covert_types::{
    entity::Entity,
    policy::{PathPolicy, Policy},
};

use crate::common::setup_unseal;

#[tokio::test]
async fn status() {
    let sdk = setup_unseal().await;

    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(covert_types::state::StorageState::Unsealed));
}

#[tokio::test]
async fn entity() {
    let sdk = setup_unseal().await;

    let name = "foo".to_string();
    let entity = sdk
        .entity
        .create(&CreateEntityParams { name: name.clone() })
        .await
        .unwrap()
        .entity;

    assert_eq!(
        entity,
        Entity {
            name,
            disabled: false
        }
    );
}

#[tokio::test]
async fn policy() {
    let sdk = setup_unseal().await;

    let name = "foo".to_string();
    let policy_raw = r#"
        path "sys/*" { 
            capabilities = ["read","update","create"] 
        }

        path "auth/userpass/*" {
            capabilities = ["delete"] 
        }
    "#;
    let policies = PathPolicy::parse(policy_raw).unwrap();
    let policy = Policy::new(name, policies);

    let created_policy = sdk
        .policy
        .create(&CreatePolicyParams {
            name: policy.name.clone(),
            policy: policy_raw.to_string(),
        })
        .await
        .unwrap()
        .policy;

    assert_eq!(created_policy, policy);
}
