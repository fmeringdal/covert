use std::{str::FromStr, time::Duration};

use clap::{Args, Subcommand};
use covert_sdk::{
    mounts::{BackendType, CreateMountParams, MountConfig, UpdateMountParams},
    Client,
};

use crate::handle_resp;

#[derive(Args, Debug)]
pub struct Secrets {
    #[clap(subcommand)]
    subcommand: SecretsSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum SecretsSubcommand {
    #[command(about = "enable secret engine")]
    Enable {
        #[arg(help = "name of secrets engine to enable")]
        name: String,
        #[arg(short, long)]
        path: Option<String>,
    },
    #[command(about = "disable secret engine")]
    Disable {
        #[arg(help = "path of secrets engine to disable")]
        path: String,
    },
    #[command(about = "tune secrets engine")]
    Tune {
        #[arg(help = "path of mount to tune")]
        path: String,
        #[arg(
            long,
            help = "the default TTL for secrets issed by this secrets engine"
        )]
        default_lease_ttl: Option<humantime::Duration>,
        #[arg(
            long,
            help = "the default TTL for secrets issed by this secrets engine"
        )]
        max_lease_ttl: Option<humantime::Duration>,
    },
    #[command(about = "list secret engines")]
    List,
}

impl Secrets {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            SecretsSubcommand::Enable { name, path } => {
                let path = path.unwrap_or_else(|| format!("{}/", name.to_lowercase()));
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
            SecretsSubcommand::Disable { path } => {
                let resp = sdk.mount.remove(&path).await;
                handle_resp(resp);
            }
            SecretsSubcommand::Tune {
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
            SecretsSubcommand::List => {
                let resp = sdk.mount.list().await.map(|mounts| mounts.secret);
                handle_resp(resp);
            }
        }
    }
}
