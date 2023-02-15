mod common;

use std::collections::HashMap;

use covert_sdk::kv::{
    CreateSecretParams, HardDeleteSecretParams, RecoverSecretParams, SoftDeleteSecretParams,
};

use crate::common::{setup_unseal, MOUNT_PATH};

#[tokio::test]
async fn soft_delete_and_recover() {
    let sdk = setup_unseal().await;

    let mut secret_data = HashMap::new();

    let versions = 5;
    let key = "foo";

    for version in 1..versions + 1 {
        let data: HashMap<_, _> = [(format!("key {version}"), format!("value {version}"))]
            .into_iter()
            .collect();

        assert!(sdk
            .kv
            .create(MOUNT_PATH, key, &CreateSecretParams { data: data.clone() })
            .await
            .is_ok());

        secret_data.insert(version, data);
    }

    // Delete version 2 and 3
    let delete_resp = sdk
        .kv
        .delete(
            MOUNT_PATH,
            key,
            &SoftDeleteSecretParams {
                versions: vec![2, 3],
            },
        )
        .await
        .unwrap();
    assert!(delete_resp.not_deleted.is_empty());

    // Check deleted versions
    for version in [2, 3] {
        let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(version)).await.unwrap();
        assert!(read_resp.data.is_none());
        assert_eq!(read_resp.metadata.deleted, true);
        assert_eq!(read_resp.metadata.destroyed, false);
        assert_eq!(read_resp.metadata.min_version, 1);
        assert_eq!(read_resp.metadata.max_version, versions);
        assert_eq!(read_resp.metadata.version, version);
    }

    // Check not deleted versions
    for version in [1, 4, 5] {
        let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(version)).await.unwrap();
        assert_eq!(
            read_resp.data.as_ref(),
            Some(secret_data.get(&version).unwrap())
        );
        assert_eq!(read_resp.metadata.deleted, false);
        assert_eq!(read_resp.metadata.destroyed, false);
        assert_eq!(read_resp.metadata.min_version, 1);
        assert_eq!(read_resp.metadata.max_version, versions);
        assert_eq!(read_resp.metadata.version, version);
    }

    // Recover deleted versions
    let recover_resp = sdk
        .kv
        .recover(
            MOUNT_PATH,
            key,
            &RecoverSecretParams {
                versions: vec![2, 3],
            },
        )
        .await
        .unwrap();
    assert!(recover_resp.not_recovered.is_empty());

    // Check that they are back
    for version in [2, 3] {
        let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(version)).await.unwrap();
        assert_eq!(
            read_resp.data.as_ref(),
            Some(secret_data.get(&version).unwrap())
        );
        assert_eq!(read_resp.metadata.deleted, false);
        assert_eq!(read_resp.metadata.destroyed, false);
        assert_eq!(read_resp.metadata.min_version, 1);
        assert_eq!(read_resp.metadata.max_version, versions);
        assert_eq!(read_resp.metadata.version, version);
    }
}

#[tokio::test]
async fn hard_delete() {
    let sdk = setup_unseal().await;

    let mut secret_data = HashMap::new();

    let versions = 5;
    let key = "foo";

    for version in 1..versions + 1 {
        let data: HashMap<_, _> = [(format!("key {version}"), format!("value {version}"))]
            .into_iter()
            .collect();

        assert!(sdk
            .kv
            .create(MOUNT_PATH, key, &CreateSecretParams { data: data.clone() })
            .await
            .is_ok());

        secret_data.insert(version, data);
    }

    // Delete version 2 and 3
    let delete_resp = sdk
        .kv
        .hard_delete(
            MOUNT_PATH,
            key,
            &HardDeleteSecretParams {
                versions: vec![2, 3],
            },
        )
        .await
        .unwrap();
    assert!(delete_resp.not_deleted.is_empty());

    // Check deleted versions
    for version in [2, 3] {
        let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(version)).await.unwrap();
        assert!(read_resp.data.is_none());
        assert_eq!(read_resp.metadata.deleted, true);
        assert_eq!(read_resp.metadata.destroyed, true);
        assert_eq!(read_resp.metadata.min_version, 1);
        assert_eq!(read_resp.metadata.max_version, versions);
        assert_eq!(read_resp.metadata.version, version);
    }

    // Check not deleted versions
    for version in [1, 4, 5] {
        let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(version)).await.unwrap();
        assert_eq!(
            read_resp.data.as_ref(),
            Some(secret_data.get(&version).unwrap())
        );
        assert_eq!(read_resp.metadata.deleted, false);
        assert_eq!(read_resp.metadata.destroyed, false);
        assert_eq!(read_resp.metadata.min_version, 1);
        assert_eq!(read_resp.metadata.max_version, versions);
        assert_eq!(read_resp.metadata.version, version);
    }

    // Try to recover hard deleted versions
    let recover_resp = sdk
        .kv
        .recover(
            MOUNT_PATH,
            key,
            &RecoverSecretParams {
                versions: vec![2, 3],
            },
        )
        .await
        .unwrap();
    // Failed to recover hard deleted versions
    assert_eq!(recover_resp.not_recovered, vec![2, 3]);

    // Check that they are still deleted
    for version in [2, 3] {
        let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(version)).await.unwrap();
        assert!(read_resp.data.is_none());
        assert_eq!(read_resp.metadata.deleted, true);
        assert_eq!(read_resp.metadata.destroyed, true);
        assert_eq!(read_resp.metadata.min_version, 1);
        assert_eq!(read_resp.metadata.max_version, versions);
        assert_eq!(read_resp.metadata.version, version);
    }
}
