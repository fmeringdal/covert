use clap::{Args, Subcommand};
use covert_sdk::{
    operator::{InitializeParams, UnsealParams},
    Client,
};

use crate::handle_resp;

#[derive(Args, Debug)]
pub struct Operator {
    #[clap(subcommand)]
    subcommand: OperatorSubcommands,
}

#[derive(Subcommand, Debug)]
pub enum OperatorSubcommands {
    #[command(about = "unseal the Covert server")]
    Unseal {
        #[arg(long, use_value_delimiter = true, value_delimiter = ',')]
        unseal_keys: Vec<String>,
    },
    #[command(about = "seal the Covert server")]
    Seal,
    #[command(about = "initialize the Covert server")]
    Init {
        #[arg(long)]
        shares: u8,
        #[arg(long)]
        threshold: u8,
    },
}

impl Operator {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            OperatorSubcommands::Init { shares, threshold } => {
                let resp = sdk
                    .operator
                    .initialize(&InitializeParams { shares, threshold })
                    .await;
                handle_resp(resp);
            }
            OperatorSubcommands::Unseal { unseal_keys } => {
                let resp = sdk
                    .operator
                    .unseal(&UnsealParams {
                        shares: unseal_keys,
                    })
                    .await;
                handle_resp(resp);
            }
            OperatorSubcommands::Seal => {
                let resp = sdk.operator.seal().await;
                handle_resp(resp);
            }
        }
    }
}
