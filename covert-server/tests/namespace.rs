mod common;

use common::setup_unseal;
use covert_sdk::{
    entity::{AttachEntityAliasParams, AttachEntityPolicyParams, CreateEntityParams, EntityAlias},
    mounts::{BackendType, CreateMountParams, MountConfig},
    namespace::CreateNamespaceParams,
    policy::CreatePolicyParams,
    userpass::{CreateUserParams, LoginParams},
};

#[tokio::test]
async fn namespace_create_and_delete() {
    let sdk = setup_unseal().await;

    // No child namespaces initially
    let resp = sdk.namespace.list().await.unwrap();
    assert!(resp.namespaces.is_empty());

    // Create foo namespace under root
    let name = "foo";
    let resp = sdk
        .namespace
        .create(&CreateNamespaceParams {
            name: name.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(resp.name, name);

    let resp = sdk.namespace.list().await.unwrap();
    assert_eq!(resp.namespaces.len(), 1);
    assert_eq!(resp.namespaces[0].name, name);

    let resp = sdk.namespace.delete(name).await.unwrap();
    assert_eq!(resp.name, name);

    let resp = sdk.namespace.list().await.unwrap();
    assert!(resp.namespaces.is_empty());
}

#[tokio::test]
async fn namespace_isolation() {
    let sdk = setup_unseal().await;

    for name in ["foo", "foobar", "bar"] {
        // Set root namespace
        sdk.set_namespace(None).await;
        let resp = sdk
            .namespace
            .create(&CreateNamespaceParams {
                name: name.to_string(),
            })
            .await
            .unwrap();
        assert_eq!(resp.name, name);

        // Now working in the context of child namespace
        sdk.set_namespace(Some(format!("root/{name}"))).await;

        let resp = sdk.namespace.list().await.unwrap();
        assert_eq!(resp.namespaces.len(), 0);

        // Create policy
        let policy_name = "foo".to_string();
        let policy_raw = r#"
        path "sys/*" { 
            capabilities = ["read","update","create"] 
        }
    "#;

        sdk.policy
            .create(&CreatePolicyParams {
                name: policy_name.clone(),
                policy: policy_raw.to_string(),
            })
            .await
            .unwrap();

        let policies = sdk.policy.list().await.unwrap();
        assert_eq!(policies.policies.len(), 1);

        // Create entity
        let entity_name = "foo".to_string();
        sdk.entity
            .create(&CreateEntityParams {
                name: entity_name.clone(),
            })
            .await
            .unwrap();

        // Create secrets engine
        sdk.mount
            .create(
                "kv/",
                &CreateMountParams {
                    config: MountConfig::default(),
                    variant: BackendType::Kv,
                },
            )
            .await
            .unwrap();

        let mounts = sdk.mount.list().await.unwrap();
        assert_eq!(mounts.auth.len(), 0);
        assert_eq!(mounts.secret.len(), 1);

        // Create auth method
        sdk.mount
            .create(
                "auth/userpass",
                &CreateMountParams {
                    config: MountConfig::default(),
                    variant: BackendType::Userpass,
                },
            )
            .await
            .unwrap();

        let mounts = sdk.mount.list().await.unwrap();
        assert_eq!(mounts.auth.len(), 1);
        assert_eq!(mounts.secret.len(), 1);
    }
}

#[tokio::test]
async fn delete_namespace_recursive() {
    let sdk = setup_unseal().await;

    let first_child = "foo";
    let resp = sdk
        .namespace
        .create(&CreateNamespaceParams {
            name: first_child.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(resp.name, first_child);

    sdk.set_namespace(Some(format!("root/{first_child}"))).await;

    let second_child = "bar";
    let resp = sdk
        .namespace
        .create(&CreateNamespaceParams {
            name: second_child.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(resp.name, second_child);

    let second_child_sibling = "barrr";
    let resp = sdk
        .namespace
        .create(&CreateNamespaceParams {
            name: second_child_sibling.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(resp.name, second_child_sibling);

    sdk.set_namespace(Some(format!("root/{first_child}"))).await;

    let third_child = "baz";
    let resp = sdk
        .namespace
        .create(&CreateNamespaceParams {
            name: third_child.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(resp.name, third_child);

    // Set namespace to root again and delete first child
    sdk.set_namespace(None).await;

    let resp = sdk.namespace.delete(first_child).await.unwrap();
    assert_eq!(resp.name, first_child);

    // No more namespaces
    let resp = sdk.namespace.list().await.unwrap();
    assert!(resp.namespaces.is_empty());
}

#[tokio::test]
async fn access_to_parent_ns_is_denied() {
    let sdk = setup_unseal().await;

    let tutorial_ns = "tutorial";
    sdk.namespace
        .create(&CreateNamespaceParams {
            name: tutorial_ns.to_string(),
        })
        .await
        .unwrap();

    sdk.set_namespace(Some(format!("root/{tutorial_ns}"))).await;

    // Create entity and policy in new namespace
    let policy_name = "admin".to_string();
    sdk.policy
        .create(&CreatePolicyParams {
            name: policy_name.clone(),
            policy: r#"path "*" { 
                capabilities = ["read","update","create","delete] 
            }"#
            .to_string(),
        })
        .await
        .unwrap();

    let policies = sdk.policy.list().await.unwrap();
    assert_eq!(policies.policies.len(), 1);

    let entity_name = "admin".to_string();
    sdk.entity
        .create(&CreateEntityParams {
            name: entity_name.clone(),
        })
        .await
        .unwrap();

    sdk.entity
        .attach_policies(&AttachEntityPolicyParams {
            name: entity_name.clone(),
            policy_names: vec![policy_name.clone()],
        })
        .await
        .unwrap();

    // Enable auth method and attach alias to entity
    let userpass_path = "auth/userpass/".to_string();
    let alias_name = "admin".to_string();
    sdk.mount
        .create(
            &userpass_path,
            &CreateMountParams {
                config: MountConfig::default(),
                variant: BackendType::Userpass,
            },
        )
        .await
        .unwrap();
    sdk.entity
        .attach_alias(&AttachEntityAliasParams {
            name: entity_name.clone(),
            aliases: vec![EntityAlias {
                mount_path: userpass_path.clone(),
                name: alias_name.clone(),
            }],
        })
        .await
        .unwrap();
    let password = "secret".to_string();
    sdk.userpass
        .create(
            &userpass_path,
            &CreateUserParams {
                username: alias_name.clone(),
                password: password.clone(),
            },
        )
        .await
        .unwrap();

    // Sign in with that user
    let token = sdk
        .userpass
        .login(
            &userpass_path,
            &LoginParams {
                username: alias_name.clone(),
                password: password.clone(),
            },
        )
        .await
        .unwrap()
        .token;
    sdk.set_token(Some(token.to_string())).await;

    // Try to mount a secret engine with new user
    sdk.mount
        .create(
            "psql/",
            &CreateMountParams {
                config: MountConfig::default(),
                variant: BackendType::Postgres,
            },
        )
        .await
        .unwrap();
    let mounts = sdk.mount.list().await.unwrap();
    assert_eq!(mounts.secret[0].path, "psql/");
    assert_eq!(mounts.auth[0].path, "auth/userpass/");

    // Try to mount a secret engine in the root namespace should not work
    sdk.set_namespace(Some("root".to_string())).await;
    assert!(sdk
        .mount
        .create(
            "psql/",
            &CreateMountParams {
                config: MountConfig::default(),
                variant: BackendType::Postgres,
            },
        )
        .await
        .unwrap_err()
        .contains("not authorized"));
}
