use clap::Args;
use tracing::info;
use tracing_error::ErrorLayer;
use tracing_subscriber::{prelude::*, EnvFilter};

#[derive(Args, Debug)]
pub struct Server {
    #[arg(short, long, default_value_t = 8080, env = "COVERT_PORT")]
    port: u16,
    #[arg(long, env = "COVERT_STORAGE_PATH")]
    storage_path: Option<String>,
    #[arg(long, env = "COVERT_SEAL_STORAGE_PATH")]
    seal_storage_path: Option<String>,
}

impl Server {
    pub async fn handle(self) {
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("hyper=off,debug"));

        let subscriber = tracing_subscriber::Registry::default()
            .with(ErrorLayer::default())
            .with(env_filter)
            .with(tracing_subscriber::fmt::Layer::default());

        // set the subscriber as the default for the application
        tracing::subscriber::set_global_default(subscriber)
            .expect("failed to setup tracing subscriber");

        let tmpdir_storage_path = tempfile::tempdir().unwrap();
        let storage_path = self.storage_path.unwrap_or_else(|| {
            info!("Starting in dev mode. All data will be erased on exit.");
            tmpdir_storage_path
                .path()
                .join("db-storage")
                .to_str()
                .unwrap()
                .to_string()
        });
        let tmpdir_seal_storage_path = tempfile::tempdir().unwrap();
        let seal_storage_path = self.seal_storage_path.unwrap_or_else(|| {
            tmpdir_seal_storage_path
                .path()
                .join("seal-storage")
                .to_str()
                .unwrap()
                .to_string()
        });

        let config = covert_system::Config {
            port: self.port,
            storage_path,
            seal_storage_path,
            port_tx: None,
        };

        covert_system::start(config).await.unwrap()
    }
}
