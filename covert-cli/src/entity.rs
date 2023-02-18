use clap::{Args, Subcommand};
use covert_sdk::{
    entity::{
        AttachEntityAliasParams, AttachEntityPolicyParams, CreateEntityParams, EntityAlias,
        RemoveEntityAliasParams, RemoveEntityPolicyParams,
    },
    Client,
};

use crate::handle_resp;

#[derive(Args, Debug)]
pub struct Entity {
    #[clap(subcommand)]
    subcommand: EntitySubcommand,
}

#[derive(Subcommand, Debug)]
pub enum EntitySubcommand {
    #[command(about = "add new entity")]
    Add {
        #[arg(short, long, help = "name of entity")]
        name: String,
    },
    #[command(about = "attach policy to entity")]
    AttachPolicy {
        #[arg(short, long, help = "name of entity")]
        name: String,
        #[arg(short, long, use_value_delimiter = true, value_delimiter = ',')]
        policies: Vec<String>,
    },
    #[command(about = "remove policy from entity")]
    RemovePolicy {
        #[arg(short, long, help = "name of entity")]
        name: String,
        #[arg(short, long)]
        policy: String,
    },
    #[command(about = "attach alias to entity")]
    AttachAlias {
        #[arg(short, long, help = "name of entity")]
        name: String,
        #[arg(short, long)]
        alias: String,
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "remove alias from entity")]
    RemoveAlias {
        #[arg(short, long, help = "name of entity")]
        name: String,
        #[arg(short, long)]
        alias: String,
        #[arg(short, long)]
        path: String,
    },
}

impl Entity {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            EntitySubcommand::Add { name } => {
                let resp = sdk.entity.create(&CreateEntityParams { name }).await;
                handle_resp(resp);
            }
            EntitySubcommand::AttachPolicy { name, policies } => {
                let resp = sdk
                    .entity
                    .attach_policies(&AttachEntityPolicyParams {
                        name,
                        policy_names: policies,
                    })
                    .await;
                handle_resp(resp);
            }
            EntitySubcommand::RemovePolicy { name, policy } => {
                let resp = sdk
                    .entity
                    .remove_policy(
                        &name,
                        &RemoveEntityPolicyParams {
                            policy_name: policy,
                        },
                    )
                    .await;
                handle_resp(resp);
            }
            EntitySubcommand::AttachAlias { name, alias, path } => {
                let resp = sdk
                    .entity
                    .attach_alias(&AttachEntityAliasParams {
                        name,
                        aliases: vec![EntityAlias {
                            name: alias,
                            mount_path: path,
                        }],
                    })
                    .await;
                handle_resp(resp);
            }
            EntitySubcommand::RemoveAlias { name, alias, path } => {
                let resp = sdk
                    .entity
                    .remove_alias(
                        &name,
                        &RemoveEntityAliasParams {
                            alias: EntityAlias {
                                name: alias,
                                mount_path: path,
                            },
                        },
                    )
                    .await;
                handle_resp(resp);
            }
        }
    }
}
