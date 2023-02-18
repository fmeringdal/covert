mod common;

use covert_sdk::entity::CreateEntityParams;
use covert_types::entity::Entity;

use common::setup_unseal;

#[tokio::test]
async fn entity() {
    let sdk = setup_unseal().await;

    let name = "foo".to_string();
    let entity = sdk
        .entity
        .create(&CreateEntityParams { name: name.clone() })
        .await
        .unwrap()
        .entity;

    assert_eq!(
        entity,
        Entity {
            name,
            disabled: false
        }
    );
}
