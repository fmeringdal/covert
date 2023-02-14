use clap::Args;

#[derive(Args, Debug)]
pub struct Server {
    #[arg(short, long, default_value_t = 8080, env = "COVERT_PORT")]
    port: u16,
    #[arg(short, long, env = "COVERT_STORAGE_PATH")]
    storage_path: Option<String>,
}

impl Server {
    pub async fn handle(self) {
        match self.storage_path {
            Some(storage_path) => {
                let config = covert_system::Config {
                    port: self.port,
                    storage_path,
                };

                covert_system::start(config).await.unwrap()
            }
            None => {
                // TODO: auto unseal
                println!("Starting in dev mode. All data will be erased on exit.");
                let tmpdir = tempfile::tempdir().unwrap();
                let storage_path = tmpdir
                    .path()
                    .join("db-storage")
                    .to_str()
                    .unwrap()
                    .to_string();

                let config = covert_system::Config {
                    port: self.port,
                    storage_path,
                };

                covert_system::start(config).await.unwrap()
            }
        }
    }
}
