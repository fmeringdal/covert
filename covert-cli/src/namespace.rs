use clap::Subcommand;
use covert_sdk::{namespace::CreateNamespaceParams, Client};

use crate::handle_resp;

#[derive(clap::Args, Debug)]
pub struct Namespace {
    #[clap(subcommand)]
    subcommand: NamespaceSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum NamespaceSubcommand {
    #[command(about = "create new namespace")]
    Create { name: String },
    #[command(about = "delete namespace recursively")]
    Delete { name: String },
    #[command(about = "list namespaces")]
    List,
}

impl Namespace {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            NamespaceSubcommand::Create { name } => {
                let resp = sdk.namespace.create(&CreateNamespaceParams { name }).await;
                handle_resp(resp);
            }
            NamespaceSubcommand::Delete { name } => {
                let resp = sdk.namespace.delete(&name).await;
                handle_resp(resp);
            }
            NamespaceSubcommand::List => {
                let resp = sdk.namespace.list().await;
                handle_resp(resp);
            }
        }
    }
}
