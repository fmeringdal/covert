use covert_sdk::{
    mounts::{BackendType, CreateMountParams, MountConfig},
    operator::{InitializeParams, InitializeResponse, UnsealParams},
    Client,
};
use tokio::sync::oneshot;

pub const MOUNT_PATH: &str = "database/";

pub async fn setup(storage: &str) -> Client {
    let (port_tx, mut port_rx) = oneshot::channel();

    let config = covert_system::Config {
        port: 0,
        port_tx: Some(port_tx),
        storage_path: storage.into(),
    };

    tokio::spawn(async move {
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
    let sdk = setup(":memory:").await;
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

    sdk.mount
        .create(
            MOUNT_PATH,
            &CreateMountParams {
                variant: BackendType::Userpass,
                config: MountConfig::default(),
            },
        )
        .await
        .unwrap();

    sdk
}