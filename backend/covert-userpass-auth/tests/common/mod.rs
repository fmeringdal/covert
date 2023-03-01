use covert_sdk::{
    mounts::{BackendType, CreateMountParams, MountConfig},
    operator::{InitializeParams, InitializeResponse, UnsealParams, UnsealResponse},
    Client,
};
use tokio::sync::oneshot;

pub const MOUNT_PATH: &str = "auth/userpass/";

pub async fn setup(storage: &str) -> Client {
    let (port_tx, port_rx) = oneshot::channel();

    let config = covert_system::Config {
        port: 0,
        port_tx: Some(port_tx),
        storage_path: storage.into(),
        replication: None,
    };

    tokio::spawn(async move {
        if let Err(err) = covert_system::start(config, covert_system::shutdown_signal()).await {
            panic!("server error: {}", err);
        }
    });

    let port = port_rx.await.unwrap();
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
    let resp = sdk.operator.unseal(&UnsealParams { shares }).await.unwrap();
    if let UnsealResponse::Complete { root_token } = resp {
        sdk.set_token(Some(root_token.to_string())).await;
    }

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
