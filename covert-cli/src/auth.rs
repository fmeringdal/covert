use std::{str::FromStr, time::Duration};

use clap::Subcommand;
use covert_sdk::{
    mounts::{BackendType, CreateMountParams, MountConfig, UpdateMountParams},
    Client,
};

use crate::handle_resp;

#[derive(clap::Args, Debug)]
pub struct Auth {
    #[clap(subcommand)]
    subcommand: AuthSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum AuthSubcommand {
    #[command(about = "enable auth method")]
    Enable {
        #[arg(help = "name of auth method to enable")]
        name: String,
        #[arg(short, long, help = "path to mount at")]
        path: Option<String>,
    },
    #[command(about = "disable auth method")]
    Disable {
        #[arg(help = "path of mount to disable")]
        path: String,
    },
    #[command(about = "tune auth method")]
    Tune {
        #[arg(help = "path of mount to tune")]
        path: String,
        #[arg(long, help = "the default TTL for token issed by this auth method")]
        default_lease_ttl: Option<humantime::Duration>,
        #[arg(long, help = "the default TTL for token issed by this auth method")]
        max_lease_ttl: Option<humantime::Duration>,
    },
    #[command(about = "list auth methods")]
    List,
}

impl Auth {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            AuthSubcommand::Enable { name, path } => {
                let path = path.unwrap_or_else(|| format!("auth/{}/", name.to_lowercase()));
                let resp = sdk
                    .mount
                    .create(
                        &path,
                        &CreateMountParams {
                            config: Default::default(),
                            variant: BackendType::from_str(&name).expect("invalid backend"),
                        },
                    )
                    .await;
                handle_resp(resp);
            }
            AuthSubcommand::Disable { path } => {
                let resp = sdk.mount.remove(&path).await;
                handle_resp(resp);
            }
            AuthSubcommand::Tune {
                path,
                default_lease_ttl,
                max_lease_ttl,
            } => {
                let mut config = MountConfig::default();
                if let Some(ttl) = default_lease_ttl {
                    config.default_lease_ttl = Duration::from_millis(ttl.as_millis() as u64);
                }
                if let Some(ttl) = max_lease_ttl {
                    config.max_lease_ttl = Duration::from_millis(ttl.as_millis() as u64);
                }

                let resp = sdk.mount.update(&path, &UpdateMountParams { config }).await;
                handle_resp(resp);
            }
            AuthSubcommand::List => {
                let resp = sdk.mount.list().await.map(|mounts| mounts.auth);
                handle_resp(resp);
            }
        }
    }
}
