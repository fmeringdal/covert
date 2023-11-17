mod common;

use std::time::Duration;

use covert_sdk::{
    entity::CreateEntityParams,
    operator::{InitializeParams, InitializeResponse, UnsealParams, UnsealResponse},
};
use covert_system::ReplicationConfig;
use covert_types::{methods::system::EntityWithPolicyAndAlias, state::StorageState};
use rand::{distributions::Alphanumeric, Rng};
use tokio::sync::oneshot;

use common::setup;

#[tokio::test]
#[cfg_attr(not(feature = "replication-integration-test"), ignore)]
async fn unseal_with_recovered_seal_config_from_local_storage() {
    let tmpdir_storage_path = tempfile::tempdir().unwrap();
    let storage_path = tmpdir_storage_path.path().to_str().unwrap().to_string();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let shutdown_rx_fut = async { shutdown_rx.await.unwrap() };

    let random_bucket_key: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    let replication = ReplicationConfig {
        access_key_id: "minioadmin".to_string(),
        secret_access_key: "minioadmin".to_string(),
        bucket: "mybkt".to_string(),
        endpoint: Some("http://localhost:9000".to_string()),
        region: "eu-west-1".to_string(),
        prefix: random_bucket_key,
    };

    let sdk = setup(&storage_path, shutdown_rx_fut, Some(replication.clone())).await;

    // Start in uninit state
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Uninitialized));

    // init
    let shares = 5;
    let threshold = 3;
    let resp = sdk
        .operator
        .initialize(&InitializeParams { shares, threshold })
        .await
        .unwrap();
    let InitializeResponse::NewKeyShares(key_shares) = resp else {
        panic!("Unexpected init response");
    };
    assert_eq!(key_shares.shares.len(), usize::from(shares));

    // unseal
    let root_token = match sdk
        .operator
        .unseal(&UnsealParams {
            shares: key_shares.shares.clone(),
        })
        .await
        .unwrap()
    {
        UnsealResponse::Complete { root_token } => root_token,
        _ => panic!("unexpected unseal response"),
    };
    sdk.set_token(Some(root_token.to_string())).await;

    // And state should now be unseal
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Unsealed));
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Shutdown
    shutdown_tx.send(()).unwrap();

    // It is down
    assert!(sdk.status.status().await.is_err());

    // Repeat seal -> unseal -> seal process a couple of times
    for _ in 0..3 {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown_rx_fut = async { shutdown_rx.await.unwrap() };

        // Start again
        let sdk = setup(&storage_path, shutdown_rx_fut, Some(replication.clone())).await;
        // Root token from previous unseal should still work
        sdk.set_token(Some(root_token.to_string())).await;

        // State should be sealed
        let resp = sdk.status.status().await.map(|resp| resp.state);
        assert_eq!(resp, Ok(StorageState::Sealed));

        // Try to unseal with invalid shares should still return error
        assert!(sdk
            .operator
            .unseal(&UnsealParams {
                shares: vec![
                    "bad key 1".to_string(),
                    "bad key 2".to_string(),
                    "bad key 3".to_string()
                ],
            })
            .await
            .is_err());

        // Try to unseal with shares given before the shutdown
        sdk.operator
            .unseal(&UnsealParams {
                shares: key_shares.shares.clone(),
            })
            .await
            .unwrap();

        // State should be unsaled
        let resp = sdk.status.status().await.map(|resp| resp.state);
        assert_eq!(resp, Ok(StorageState::Unsealed));

        // Just to clean up all replication subprocesses
        shutdown_tx.send(()).expect("shutdown cleanly");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "replication-integration-test"), ignore)]
async fn unseal_with_recovered_seal_config_from_remote_backup() {
    let tmpdir_storage_path = tempfile::tempdir().unwrap();
    let storage_path = tmpdir_storage_path.path().to_str().unwrap().to_string();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let shutdown_rx_fut = async { shutdown_rx.await.unwrap() };

    let random_bucket_key: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    let replication = ReplicationConfig {
        access_key_id: "minioadmin".to_string(),
        secret_access_key: "minioadmin".to_string(),
        bucket: "mybkt".to_string(),
        endpoint: Some("http://localhost:9000".to_string()),
        region: "eu-west-1".to_string(),
        prefix: random_bucket_key,
    };

    let sdk = setup(&storage_path, shutdown_rx_fut, Some(replication.clone())).await;

    // Start in uninit state
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Uninitialized));

    // init
    let shares = 5;
    let threshold = 3;
    let resp = sdk
        .operator
        .initialize(&InitializeParams { shares, threshold })
        .await
        .unwrap();
    let InitializeResponse::NewKeyShares(key_shares) = resp else {
        panic!("Unexpected init response");
    };
    assert_eq!(key_shares.shares.len(), usize::from(shares));

    // unseal
    let root_token = match sdk
        .operator
        .unseal(&UnsealParams {
            shares: key_shares.shares.clone(),
        })
        .await
        .unwrap()
    {
        UnsealResponse::Complete { root_token } => root_token,
        _ => panic!("unexpected unseal response"),
    };
    sdk.set_token(Some(root_token.to_string())).await;

    // Wait for replication
    tokio::time::sleep(Duration::from_secs(2)).await;

    // And state should now be unseal
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Unsealed));

    // Shutdown
    shutdown_tx.send(()).unwrap();

    // Delete local storage
    std::fs::remove_dir_all(tmpdir_storage_path.path()).unwrap();
    assert!(!std::path::Path::new(tmpdir_storage_path.path()).exists());

    // It is down
    assert!(sdk.status.status().await.is_err());

    for _ in 0..3 {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown_rx_fut = async { shutdown_rx.await.unwrap() };

        // Start again
        let sdk = setup(&storage_path, shutdown_rx_fut, Some(replication.clone())).await;
        // Root token from previous unseal should still work
        sdk.set_token(Some(root_token.to_string())).await;

        // State should be sealed
        let resp = sdk.status.status().await.map(|resp| resp.state);
        assert_eq!(resp, Ok(StorageState::Sealed));

        // Try to unseal with invalid shares should still return error
        assert!(sdk
            .operator
            .unseal(&UnsealParams {
                shares: vec![
                    "bad key 1".to_string(),
                    "bad key 2".to_string(),
                    "bad key 3".to_string()
                ],
            })
            .await
            .is_err());

        // Try to unseal with shares given before the shutdown
        sdk.operator
            .unseal(&UnsealParams {
                shares: key_shares.shares.clone(),
            })
            .await
            .unwrap();

        // State should be unsaled
        let resp = sdk.status.status().await.map(|resp| resp.state);
        assert_eq!(resp, Ok(StorageState::Unsealed));

        // Just to clean up all replication subprocesses
        shutdown_tx.send(()).expect("shutdown cleanly");
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Delete local storage
        std::fs::remove_dir_all(tmpdir_storage_path.path()).unwrap();
        assert!(!std::path::Path::new(tmpdir_storage_path.path()).exists());

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "replication-integration-test"), ignore)]
async fn recover_encrypted_db_from_backup() {
    let tmpdir_storage_path = tempfile::tempdir().unwrap();
    let storage_path = tmpdir_storage_path.path().to_str().unwrap().to_string();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let shutdown_rx_fut = async { shutdown_rx.await.unwrap() };

    let random_bucket_key: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    let replication = ReplicationConfig {
        access_key_id: "minioadmin".to_string(),
        secret_access_key: "minioadmin".to_string(),
        bucket: "mybkt".to_string(),
        endpoint: Some("http://localhost:9000".to_string()),
        region: "eu-west-1".to_string(),
        prefix: random_bucket_key,
    };

    let sdk = setup(&storage_path, shutdown_rx_fut, Some(replication.clone())).await;

    // Start in uninit state
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Uninitialized));

    // init
    let shares = 5;
    let threshold = 3;
    let resp = sdk
        .operator
        .initialize(&InitializeParams { shares, threshold })
        .await
        .unwrap();
    let InitializeResponse::NewKeyShares(key_shares) = resp else {
        panic!("Unexpected init response");
    };
    assert_eq!(key_shares.shares.len(), usize::from(shares));

    // unseal
    let root_token = match sdk
        .operator
        .unseal(&UnsealParams {
            shares: key_shares.shares.clone(),
        })
        .await
        .unwrap()
    {
        UnsealResponse::Complete { root_token } => root_token,
        _ => panic!("unexpected unseal response"),
    };
    sdk.set_token(Some(root_token.to_string())).await;

    // And state should now be unseal
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Unsealed));

    // Insert couple of entities
    let john = "john".to_string();
    sdk.entity
        .create(&CreateEntityParams { name: john.clone() })
        .await
        .unwrap();
    let james = "james".to_string();
    sdk.entity
        .create(&CreateEntityParams {
            name: james.clone(),
        })
        .await
        .unwrap();

    // Wait 2 sec until replication should have picked up
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Shutdown
    shutdown_tx.send(()).unwrap();

    // Delete local storage
    std::fs::remove_dir_all(tmpdir_storage_path.path()).unwrap();
    assert!(!std::path::Path::new(tmpdir_storage_path.path()).exists());

    // It is down
    assert!(sdk.status.status().await.is_err());

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let shutdown_rx_fut = async { shutdown_rx.await.unwrap() };

    // Start again
    let sdk = setup(&storage_path, shutdown_rx_fut, Some(replication.clone())).await;
    // Root token from previous unseal should still work
    sdk.set_token(Some(root_token.to_string())).await;

    // State should be sealed
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Sealed));

    // Try to unseal with shares given before the shutdown
    sdk.operator
        .unseal(&UnsealParams {
            shares: key_shares.shares.clone(),
        })
        .await
        .unwrap();

    // State should be unsaled
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Unsealed));

    // Check that entities are there
    let entities = sdk.entity.list().await.unwrap();
    assert_eq!(
        entities.entities,
        vec![
            EntityWithPolicyAndAlias {
                name: james.to_string(),
                aliases: vec![],
                policies: vec![]
            },
            EntityWithPolicyAndAlias {
                name: john.to_string(),
                aliases: vec![],
                policies: vec![]
            },
            EntityWithPolicyAndAlias {
                name: "root".to_string(),
                aliases: vec![],
                policies: vec!["root".to_string()]
            },
        ]
    );

    // Insert one more entity and repeat cycle
    let foo_name = "foo".to_string();
    sdk.entity
        .create(&CreateEntityParams {
            name: foo_name.clone(),
        })
        .await
        .unwrap();

    // Wait 2 sec until replication should have picked up
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Shutdown
    shutdown_tx.send(()).unwrap();

    // Delete local storage
    std::fs::remove_dir_all(tmpdir_storage_path.path()).unwrap();
    assert!(!std::path::Path::new(tmpdir_storage_path.path()).exists());

    // It is down
    assert!(sdk.status.status().await.is_err());

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let shutdown_rx_fut = async { shutdown_rx.await.unwrap() };

    // Start again
    let sdk = setup(&storage_path, shutdown_rx_fut, Some(replication.clone())).await;
    // Root token from previous unseal should still work
    sdk.set_token(Some(root_token.to_string())).await;

    // Unseal
    sdk.operator
        .unseal(&UnsealParams {
            shares: key_shares.shares.clone(),
        })
        .await
        .unwrap();

    // Check that entities are there
    let entities = sdk.entity.list().await.unwrap();
    assert_eq!(
        entities.entities,
        vec![
            EntityWithPolicyAndAlias {
                name: foo_name.to_string(),
                aliases: vec![],
                policies: vec![]
            },
            EntityWithPolicyAndAlias {
                name: james.to_string(),
                aliases: vec![],
                policies: vec![]
            },
            EntityWithPolicyAndAlias {
                name: john.to_string(),
                aliases: vec![],
                policies: vec![]
            },
            EntityWithPolicyAndAlias {
                name: "root".to_string(),
                aliases: vec![],
                policies: vec!["root".to_string()]
            },
        ]
    );

    // Insert one more entity and repeat cycle again
    let bar_name = "bar".to_string();
    sdk.entity
        .create(&CreateEntityParams {
            name: bar_name.clone(),
        })
        .await
        .unwrap();

    // Wait 2 sec until replication should have picked up
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Shutdown and don't wait for replication and don't delete local storage
    shutdown_tx.send(()).expect("shutdown cleanly");

    // It is down
    assert!(sdk.status.status().await.is_err());

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let shutdown_rx_fut = async { shutdown_rx.await.unwrap() };

    // Start again
    let sdk = setup(&storage_path, shutdown_rx_fut, Some(replication)).await;
    // Root token from previous unseal should still work
    sdk.set_token(Some(root_token.to_string())).await;

    // Unseal
    sdk.operator
        .unseal(&UnsealParams {
            shares: key_shares.shares.clone(),
        })
        .await
        .unwrap();

    // Check that entities are there
    let entities = sdk.entity.list().await.unwrap();
    assert_eq!(
        entities.entities,
        vec![
            EntityWithPolicyAndAlias {
                name: bar_name.to_string(),
                aliases: vec![],
                policies: vec![]
            },
            EntityWithPolicyAndAlias {
                name: foo_name.to_string(),
                aliases: vec![],
                policies: vec![]
            },
            EntityWithPolicyAndAlias {
                name: james.to_string(),
                aliases: vec![],
                policies: vec![]
            },
            EntityWithPolicyAndAlias {
                name: john.to_string(),
                aliases: vec![],
                policies: vec![]
            },
            EntityWithPolicyAndAlias {
                name: "root".to_string(),
                aliases: vec![],
                policies: vec!["root".to_string()]
            },
        ]
    );

    // Just to clean up all replication subprocesses
    shutdown_tx.send(()).expect("shutdown cleanly");
}
