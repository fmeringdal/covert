mod common;

use common::setup_unseal;

#[tokio::test]
async fn status() {
    let sdk = setup_unseal().await;

    let resp = sdk.status.status().await.map(|resp| resp.state);
    assert_eq!(resp, Ok(covert_types::state::StorageState::Unsealed));
}
