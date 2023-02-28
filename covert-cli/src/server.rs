use clap::Args;
use covert_system::Config;
use tracing::info;
use tracing_error::ErrorLayer;
use tracing_subscriber::{prelude::*, EnvFilter};

#[derive(Args, Debug)]
pub struct Server {
    #[arg(short, long)]
    config: String,
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

        let config_file = std::fs::read_to_string(&self.config).expect("failed to read config");
        let mut config: Config = toml::from_str(&config_file).expect("failed to parse config file");

        let tmpdir_storage_path = tempfile::tempdir().unwrap();
        config.storage_path = if config.storage_path.is_empty() {
            info!("Starting in dev mode. All data will be erased on exit.");
            tmpdir_storage_path.path().to_str().unwrap().to_string()
        } else {
            config.storage_path.clone()
        };

        covert_system::start(config, covert_system::shutdown_signal())
            .await
            .unwrap()
    }
}
