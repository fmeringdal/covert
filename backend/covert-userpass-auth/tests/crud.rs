mod common;

use covert_sdk::{
    entity::{AttachEntityAliasParams, CreateEntityParams, EntityAlias},
    userpass::{CreateUserParams, LoginParams, UpdateUserPasswordParams},
};
use covert_types::methods::userpass::UserListItem;

use crate::common::{setup_unseal, MOUNT_PATH};

pub const CONNECTION_STR: &str =
    "postgresql://root:rootpassword@127.0.0.1:5432/postgres?sslmode=disable";

#[tokio::test]
async fn crud() {
    let sdk = setup_unseal().await;

    // No connection to start with
    let username = "foo";
    let password = "foo_pass";
    let new_password = "foo_pass_new";
    let resp = sdk
        .userpass
        .create(
            MOUNT_PATH,
            &CreateUserParams {
                username: username.to_string(),
                password: password.to_string(),
            },
        )
        .await
        .unwrap();
    assert_eq!(resp.username, username);

    // Setup entity with alias
    let entity_name = "foo_entity_name";
    sdk.entity
        .create(&CreateEntityParams {
            name: entity_name.to_string(),
        })
        .await
        .unwrap();
    let resp = sdk
        .entity
        .attach_alias(&AttachEntityAliasParams {
            name: entity_name.to_string(),
            aliases: vec![EntityAlias {
                name: username.to_string(),
                mount_path: MOUNT_PATH.to_string(),
            }],
        })
        .await
        .unwrap();
    assert_eq!(resp.aliases.len(), 1);

    let resp = sdk.userpass.list(MOUNT_PATH).await.unwrap();
    assert_eq!(
        resp.users,
        vec![UserListItem {
            username: username.to_string()
        }]
    );

    // Sign in with correct password works
    let resp = sdk
        .userpass
        .login(
            MOUNT_PATH,
            &LoginParams {
                username: username.to_string(),
                password: password.to_string(),
            },
        )
        .await;
    assert!(resp.is_ok());

    // Sign in with invalid password does not work
    let resp = sdk
        .userpass
        .update_password(
            MOUNT_PATH,
            username,
            &UpdateUserPasswordParams {
                password: "invalid".to_string(),
                new_password: new_password.to_string(),
            },
        )
        .await;
    assert!(resp.is_err());

    let resp = sdk
        .userpass
        .update_password(
            MOUNT_PATH,
            username,
            &UpdateUserPasswordParams {
                password: password.to_string(),
                new_password: new_password.to_string(),
            },
        )
        .await;
    assert!(resp.is_ok());

    // Login with old password does not work
    let resp = sdk
        .userpass
        .login(
            MOUNT_PATH,
            &LoginParams {
                username: username.to_string(),
                password: password.to_string(),
            },
        )
        .await;
    assert!(resp.is_err());

    // Login with new password works
    let resp = sdk
        .userpass
        .login(
            MOUNT_PATH,
            &LoginParams {
                username: username.to_string(),
                password: new_password.to_string(),
            },
        )
        .await;
    assert!(resp.is_ok());

    let resp = sdk.userpass.remove(MOUNT_PATH, username).await.unwrap();
    assert_eq!(resp.username, username);

    let resp = sdk.userpass.list(MOUNT_PATH).await.unwrap();
    assert!(resp.users.is_empty());

    // Login after user removed does not work
    let resp = sdk
        .userpass
        .login(
            MOUNT_PATH,
            &LoginParams {
                username: username.to_string(),
                password: new_password.to_string(),
            },
        )
        .await;
    assert!(resp.is_err());
}
