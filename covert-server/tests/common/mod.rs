use covert_sdk::{
    operator::{InitializeParams, InitializeResponse, UnsealParams},
    Client,
};
use tokio::sync::oneshot;

pub async fn setup() -> Client {
    let (port_tx, mut port_rx) = oneshot::channel();

    let config = covert_system::Config {
        port: 0,
        port_tx: Some(port_tx),
        storage_path: ":memory:".into(),
    };

    tokio::spawn(async move {
        // server.with;
        if let Err(err) = covert_system::start(config).await {
            panic!("server error: {}", err);
        }
    });
    tokio::task::yield_now().await;

    let port = port_rx.try_recv().unwrap();
    let sdk = Client::new(format!("http://localhost:{port}/v1"));

    sdk
}

pub async fn setup_unseal() -> Client {
    let sdk = setup().await;
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
    sdk.operator.unseal(&UnsealParams { shares }).await.unwrap();
    sdk
}
