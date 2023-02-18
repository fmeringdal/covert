mod common;

use covert_sdk::operator::{InitializeParams, InitializeResponse, UnsealParams, UnsealResponse};
use covert_types::state::StorageState;

use common::setup;
use tokio::sync::oneshot;

#[tokio::test]
async fn seal() {
    let sdk = setup(":memory:", ":memory:", covert_system::shutdown_signal()).await;

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

    // Should be in sealed state after init
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Sealed));

    // Init again fails
    assert!(sdk
        .operator
        .initialize(&InitializeParams { shares, threshold })
        .await
        .is_err());

    // Start unseal
    for i in 0..usize::from(threshold) {
        let resp = sdk
            .operator
            .unseal(&UnsealParams {
                shares: vec![key_shares.shares[i].clone()],
            })
            .await
            .unwrap();

        if u8::try_from(i).unwrap() == threshold - 1 {
            assert!(matches!(resp, UnsealResponse::Complete { .. }));
        } else {
            assert!(matches!(
                resp,
                UnsealResponse::InProgress {
                    threshold: returned_threshold,
                    key_shares_provided,
                    key_shares_total
                } if threshold == returned_threshold && key_shares_provided == i+1 && key_shares_total == shares
            ));

            // And state is still sealed
            let resp = sdk.status.status().await.map(|resp| resp.state);
            assert_eq!(resp, Ok(StorageState::Sealed));
        }
    }

    // And state should now be unseal
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Unsealed));

    // Unseal while unsealed returns error
    assert!(sdk
        .operator
        .unseal(&UnsealParams {
            shares: key_shares.shares.clone(),
        })
        .await
        .is_err());

    // Seal again
    assert!(sdk.operator.seal().await.is_ok());
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Sealed));

    // Try to unseal with same key multiple times should fail
    for _ in 0..usize::from(threshold) {
        let resp = sdk
            .operator
            .unseal(&UnsealParams {
                shares: vec![key_shares.shares[0].clone()],
            })
            .await
            .unwrap();

        assert!(matches!(
            resp,
            UnsealResponse::InProgress {
                threshold: returned_threshold,
                key_shares_provided,
                key_shares_total
            } if threshold == returned_threshold && key_shares_provided == 1 && key_shares_total == shares
        ));

        // And state is still sealed
        let resp = sdk.status.status().await.map(|resp| resp.state);
        assert_eq!(resp, Ok(StorageState::Sealed));
    }

    // Key shares are cleared when threshold is reached and it was unable to unseal
    assert!(sdk
        .operator
        .unseal(&UnsealParams {
            shares: vec!["Bad key 1".to_string(), "Bad key 2".to_string()],
        })
        .await
        .is_err());

    // And state is still sealed
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Sealed));

    // Start unseal again
    for i in 0..usize::from(threshold) {
        let resp = sdk
            .operator
            .unseal(&UnsealParams {
                shares: vec![key_shares.shares[i].clone()],
            })
            .await
            .unwrap();

        if u8::try_from(i).unwrap() == threshold - 1 {
            assert!(matches!(resp, UnsealResponse::Complete { .. }));
        } else {
            assert!(matches!(
                resp,
                UnsealResponse::InProgress {
                    threshold: returned_threshold,
                    key_shares_provided,
                    key_shares_total
                } if threshold == returned_threshold && key_shares_provided == i+1 && key_shares_total == shares
            ));

            // And state is still sealed
            let resp = sdk.status.status().await.map(|resp| resp.state);
            assert_eq!(resp, Ok(StorageState::Sealed));
        }
    }

    // And state should now be unseal
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Unsealed));
}

#[tokio::test]
async fn recover_seal_config_after_shutdown() {
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
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let shutdown_rx_fut = async { shutdown_rx.await.unwrap() };

    let sdk = setup(&storage_path, &seal_config_path, shutdown_rx_fut).await;

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
    sdk.operator
        .unseal(&UnsealParams {
            shares: key_shares.shares.clone(),
        })
        .await
        .unwrap();

    // And state should now be unseal
    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(StorageState::Unsealed));

    // Shutdown
    shutdown_tx.send(()).unwrap();

    // It is down
    assert!(sdk.status.status().await.is_err());

    // Start again
    let sdk = setup(
        &storage_path,
        &seal_config_path,
        covert_system::shutdown_signal(),
    )
    .await;

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
}
