use covert_sdk::{
    operator::{InitializeParams, InitializeResponse, UnsealParams, UnsealResponse},
    Client,
};
use covert_system::ReplicationConfig;
use tokio::sync::oneshot;

use std::future::Future;

pub async fn setup(
    storage_path: &str,
    shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static,
    replication: Option<ReplicationConfig>,
) -> Client {
    let (port_tx, port_rx) = oneshot::channel();

    let config = covert_system::Config {
        port: 0,
        port_tx: Some(port_tx),
        storage_path: storage_path.into(),
        replication,
    };

    tokio::spawn(async move {
        if let Err(err) = covert_system::start(config, shutdown_signal).await {
            panic!("server error: {}", err);
        }
    });

    let port = port_rx.await.unwrap();
    let sdk = Client::new(format!("http://localhost:{port}/v1"));

    sdk
}

#[allow(dead_code)]
pub async fn setup_unseal() -> Client {
    let sdk = setup(":memory:", covert_system::shutdown_signal(), None).await;
    let shares = match sdk
        .operator
        .initialize(&InitializeParams {
            shares: 1,
            threshold: 1,
        })
        .await
        .unwrap()
    {
        InitializeResponse::NewKeyShares(shares) => shares.shares,
        _ => panic!("should get new shares"),
    };
    let resp = sdk.operator.unseal(&UnsealParams { shares }).await.unwrap();
    if let UnsealResponse::Complete { root_token } = resp {
        sdk.set_token(Some(root_token.to_string())).await;
    }

    sdk
}
