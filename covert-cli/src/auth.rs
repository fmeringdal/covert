use std::str::FromStr;

use clap::Subcommand;
use covert_sdk::{
    mounts::{BackendType, CreateMountParams},
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
    #[command(about = "list auth methods")]
    List,
}

impl Auth {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            AuthSubcommand::Enable { name, path } => {
                let path = path.unwrap_or_else(|| format!("auth/{name}/"));
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
            AuthSubcommand::List => {
                let resp = sdk.mount.list().await.map(|mounts| mounts.auth);
                handle_resp(resp);
            }
        }
    }
}
