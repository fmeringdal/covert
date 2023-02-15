mod common;

use std::collections::HashMap;

use covert_sdk::kv::{CreateSecretParams, SetConfigParams};

use crate::common::{setup_unseal, MOUNT_PATH};

#[tokio::test]
async fn max_versions() {
    let sdk = setup_unseal().await;

    // Read default config
    let resp = sdk.kv.read_config(MOUNT_PATH).await.unwrap();
    assert_eq!(resp.max_versions, 10);

    // Update max versions
    let max_versions = 5;
    let resp = sdk
        .kv
        .set_config(MOUNT_PATH, &SetConfigParams { max_versions })
        .await
        .unwrap();
    assert_eq!(resp.max_versions, max_versions);

    // Read config
    let resp = sdk.kv.read_config(MOUNT_PATH).await.unwrap();
    assert_eq!(resp.max_versions, max_versions);

    // Create max versions of a key
    let key = "foo";
    for version in 1..max_versions + 1 {
        let data: HashMap<_, _> = [(format!("key {version}"), format!("value {version}"))]
            .into_iter()
            .collect();

        assert!(sdk
            .kv
            .create(MOUNT_PATH, key, &CreateSecretParams { data: data.clone() })
            .await
            .is_ok());
    }

    // All versions should be present
    let read_resp = sdk.kv.read(MOUNT_PATH, key, None).await.unwrap();
    assert_eq!(read_resp.metadata.min_version, 1);
    assert_eq!(read_resp.metadata.max_version, max_versions);
    assert_eq!(read_resp.metadata.version, max_versions);

    // Create one more version for the key
    let data: HashMap<_, _> = [("key".to_string(), "value".to_string())]
        .into_iter()
        .collect();

    assert!(sdk
        .kv
        .create(MOUNT_PATH, key, &CreateSecretParams { data: data.clone() })
        .await
        .is_ok());

    // Version 1 should now be gone
    let read_resp = sdk.kv.read(MOUNT_PATH, key, None).await.unwrap();
    assert_eq!(read_resp.metadata.min_version, 2);
    assert_eq!(read_resp.metadata.max_version, max_versions + 1);
    assert_eq!(read_resp.metadata.version, max_versions + 1);

    // Try to read version 1
    let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(1)).await;
    assert_eq!(
        read_resp.unwrap_err(),
        "A key with that version was not found"
    );
}
