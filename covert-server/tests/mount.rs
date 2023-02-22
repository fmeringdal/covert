mod common;

use std::time::Duration;

use common::{setup, setup_unseal};
use covert_sdk::{
    mounts::{BackendType, CreateMountParams, MountConfig, UpdateMountParams},
    operator::{InitializeParams, InitializeResponse, UnsealParams, UnsealResponse},
};
use covert_types::state::StorageState;

#[tokio::test]
async fn mount() {
    let sdk = setup_unseal().await;

    // Initial mounts
    let mounts = sdk.mount.list().await.unwrap();
    assert_eq!(mounts.auth.len(), 0);
    assert_eq!(mounts.secret.len(), 0);

    // Mount kv secret engine
    let mut mount_config = MountConfig {
        max_lease_ttl: Duration::from_secs(3600 * 24 * 365),
        ..Default::default()
    };
    sdk.mount
        .create(
            "kv/",
            &CreateMountParams {
                config: mount_config.clone(),
                variant: BackendType::Kv,
            },
        )
        .await
        .unwrap();

    let mounts = sdk.mount.list().await.unwrap();
    assert_eq!(mounts.auth.len(), 0);
    assert_eq!(mounts.secret.len(), 1);
    assert_eq!(mounts.secret[0].variant, BackendType::Kv);
    assert_eq!(mounts.secret[0].config, mount_config);

    // Mount again under conflicting path returns error
    assert!(sdk
        .mount
        .create(
            "kv/nested/",
            &CreateMountParams {
                config: mount_config.clone(),
                variant: BackendType::Kv,
            },
        )
        .await
        .is_err());

    // Update mount
    mount_config.max_lease_ttl -= Duration::from_secs(3600);
    sdk.mount
        .update(
            "kv/",
            &UpdateMountParams {
                config: mount_config.clone(),
            },
        )
        .await
        .unwrap();
    let mounts = sdk.mount.list().await.unwrap();
    assert_eq!(mounts.auth.len(), 0);
    assert_eq!(mounts.secret.len(), 1);
    assert_eq!(mounts.secret[0].variant, BackendType::Kv);
    assert_eq!(mounts.secret[0].config, mount_config);

    // Disable mount
    sdk.mount.remove("kv/").await.unwrap();
    let mounts = sdk.mount.list().await.unwrap();
    assert_eq!(mounts.auth.len(), 0);
    assert_eq!(mounts.secret.len(), 0);
}

#[tokio::test]
async fn recover_mounts_after_seal() {
    let tmpdir_storage_path = tempfile::tempdir().unwrap();
    let tmpdir_seal_storage_path = tempfile::tempdir().unwrap();
    let storage_path = tmpdir_storage_path
        .path()
        .join("seal-config")
        .to_str()
        .unwrap()
        .to_string();
    let seal_config_path = tmpdir_seal_storage_path
        .path()
        .join("seal-config")
        .to_str()
        .unwrap()
        .to_string();
    let sdk = setup(
        &storage_path,
        &seal_config_path,
        covert_system::shutdown_signal(),
    )
    .await;
    let resp = sdk
        .operator
        .initialize(&InitializeParams {
            shares: 1,
            threshold: 1,
        })
        .await
        .unwrap();
    let InitializeResponse::NewKeyShares(key_shares) = resp else {
        panic!("Unexpected init response");
    };

    // unseal
    let resp = sdk
        .operator
        .unseal(&UnsealParams {
            shares: key_shares.shares.clone(),
        })
        .await
        .unwrap();
    if let UnsealResponse::Complete { root_token } = resp {
        sdk.set_token(Some(root_token.to_string())).await;
    }

    // Mount kv secret engine
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
    sdk.mount
        .create(
            "database/psql-marketing/",
            &CreateMountParams {
                config: MountConfig::default(),
                variant: BackendType::Postgres,
            },
        )
        .await
        .unwrap();
    sdk.mount
        .create(
            "database/psql-playground/",
            &CreateMountParams {
                config: MountConfig::default(),
                variant: BackendType::Postgres,
            },
        )
        .await
        .unwrap();

    let mounts = sdk.mount.list().await.unwrap();
    assert_eq!(mounts.auth.len(), 0);
    assert_eq!(mounts.secret.len(), 3);

    // Seal
    sdk.operator.seal().await.unwrap();
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Sealed));

    // Listing mounts in sealed state does not work
    assert!(sdk.mount.list().await.is_err());

    // Unseal again
    sdk.operator
        .unseal(&UnsealParams {
            shares: key_shares.shares.clone(),
        })
        .await
        .unwrap();
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Unsealed));
    let mounts = sdk.mount.list().await.unwrap();
    assert_eq!(mounts.auth.len(), 0);
    assert_eq!(mounts.secret.len(), 3);
}
