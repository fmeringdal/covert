mod common;

use std::collections::HashMap;

use chrono::{Duration, Utc};
use covert_sdk::kv::CreateSecretParams;

use crate::common::{setup_unseal, MOUNT_PATH};

#[tokio::test]
async fn create_and_read() {
    let sdk = setup_unseal().await;

    let now = Utc::now();
    let threshold = Duration::seconds(2);

    let key = "foo";
    let data_v1: HashMap<_, _> = [
        ("foo".to_string(), "123".to_string()),
        ("bar".to_string(), "456".to_string()),
    ]
    .into_iter()
    .collect();

    assert!(sdk
        .kv
        .create(
            MOUNT_PATH,
            key,
            &CreateSecretParams {
                data: data_v1.clone()
            }
        )
        .await
        .is_ok());

    // Read without version
    let read_resp = sdk.kv.read(MOUNT_PATH, key, None).await.unwrap();
    assert_eq!(read_resp.data, Some(data_v1.clone()));
    assert_eq!(read_resp.metadata.deleted, false);
    assert_eq!(read_resp.metadata.destroyed, false);
    assert_eq!(read_resp.metadata.min_version, 1);
    assert_eq!(read_resp.metadata.max_version, 1);
    assert_eq!(read_resp.metadata.version, 1);
    assert!(read_resp.metadata.created_time - now < threshold);

    // Read with explicit version
    let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(1)).await.unwrap();
    assert_eq!(read_resp.data, Some(data_v1.clone()));
    assert_eq!(read_resp.metadata.deleted, false);
    assert_eq!(read_resp.metadata.destroyed, false);
    assert_eq!(read_resp.metadata.min_version, 1);
    assert_eq!(read_resp.metadata.max_version, 1);
    assert_eq!(read_resp.metadata.version, 1);

    // Read version that does not exist
    let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(2)).await;
    assert_eq!(
        read_resp.unwrap_err(),
        "A key with that version was not found"
    );

    // Read key that does not exist
    let read_resp = sdk.kv.read(MOUNT_PATH, "badkey", None).await;
    assert_eq!(
        read_resp.unwrap_err(),
        "A key with that version was not found"
    );

    // Create new version
    let data_v2: HashMap<_, _> = [
        ("foo v2".to_string(), "123 v2".to_string()),
        ("bar v2".to_string(), "456 v2".to_string()),
        ("zoo v2".to_string(), "789 v2".to_string()),
    ]
    .into_iter()
    .collect();
    assert!(sdk
        .kv
        .create(
            MOUNT_PATH,
            key,
            &CreateSecretParams {
                data: data_v2.clone()
            }
        )
        .await
        .is_ok());

    // Read without version
    let read_resp = sdk.kv.read(MOUNT_PATH, key, None).await.unwrap();
    assert_eq!(read_resp.data, Some(data_v2.clone()));
    assert_eq!(read_resp.metadata.deleted, false);
    assert_eq!(read_resp.metadata.destroyed, false);
    assert_eq!(read_resp.metadata.min_version, 1);
    assert_eq!(read_resp.metadata.max_version, 2);
    assert_eq!(read_resp.metadata.version, 2);

    // Read with explicit version
    let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(2)).await.unwrap();
    assert_eq!(read_resp.data, Some(data_v2.clone()));
    assert_eq!(read_resp.metadata.deleted, false);
    assert_eq!(read_resp.metadata.destroyed, false);
    assert_eq!(read_resp.metadata.min_version, 1);
    assert_eq!(read_resp.metadata.max_version, 2);
    assert_eq!(read_resp.metadata.version, 2);

    // Read version 1 still possible
    let read_resp = sdk.kv.read(MOUNT_PATH, key, Some(1)).await.unwrap();
    assert_eq!(read_resp.data, Some(data_v1.clone()));
    assert_eq!(read_resp.metadata.deleted, false);
    assert_eq!(read_resp.metadata.destroyed, false);
    assert_eq!(read_resp.metadata.min_version, 1);
    assert_eq!(read_resp.metadata.max_version, 2);
    assert_eq!(read_resp.metadata.version, 1);
}
