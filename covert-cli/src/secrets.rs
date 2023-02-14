use std::str::FromStr;

use clap::{Args, Subcommand};
use covert_sdk::{
    mounts::{BackendType, CreateMountParams},
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
    #[command(about = "list secret engines")]
    List,
}

impl Secrets {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            SecretsSubcommand::Enable { name, path } => {
                let path = path.unwrap_or_else(|| format!("{name}/"));
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
            SecretsSubcommand::List => {
                let resp = sdk.mount.list().await.map(|mounts| mounts.secret);
                handle_resp(resp);
            }
        }
    }
}
