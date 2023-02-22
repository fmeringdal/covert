mod common;

use covert_sdk::policy::CreatePolicyParams;
use covert_types::policy::{PathPolicy, Policy};

use common::setup_unseal;

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
    let policy = Policy::new(name, policies, "foo".to_string());

    let created_policy = sdk
        .policy
        .create(&CreatePolicyParams {
            name: policy.name.clone(),
            policy: policy_raw.to_string(),
        })
        .await
        .unwrap()
        .policy;

    assert_eq!(created_policy.name, policy.name);
    assert_eq!(created_policy.paths, policy.paths);
}
