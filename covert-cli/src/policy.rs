use clap::{Args, Subcommand};
use covert_sdk::{policy::CreatePolicyParams, Client};

use crate::handle_resp;

#[derive(Args, Debug)]
pub struct Policy {
    #[clap(subcommand)]
    subcommand: PolicySubcommands,
}

#[derive(Subcommand, Debug)]
pub enum PolicySubcommands {
    #[command(about = "add new policy")]
    Add {
        #[arg(short, long, help = "name of policy to create")]
        name: String,
        #[arg(long)]
        policy: String,
    },
    #[command(about = "remove policy")]
    Remove {
        #[arg(help = "name of policy to remove")]
        name: String,
    },
    #[command(about = "list policies")]
    List,
}

impl Policy {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            PolicySubcommands::Add { name, policy } => {
                let resp = sdk
                    .policy
                    .create(&CreatePolicyParams { name, policy })
                    .await;
                handle_resp(resp);
            }
            PolicySubcommands::Remove { name } => {
                let resp = sdk.policy.remove(&name).await;
                handle_resp(resp);
            }
            PolicySubcommands::List => {
                let resp = sdk.policy.list().await;
                handle_resp(resp);
            }
        }
    }
}
