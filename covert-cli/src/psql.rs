use std::time::Duration;

use clap::{Args, Subcommand};
use covert_sdk::{
    psql::{CreateRoleParams, SetConnectionParams},
    Client,
};

use crate::handle_resp;

#[derive(Args, Debug)]
pub struct Psql {
    #[clap(subcommand)]
    subcommand: PsqlSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum PsqlSubcommand {
    #[command(about = "set connection config")]
    SetConnection {
        #[arg(help = "path where PSQL engine is mounted")]
        path: String,
        #[arg(long)]
        connection_url: String,
    },
    #[command(about = "read connection config")]
    Connection {
        #[arg(help = "path where PSQL engine is mounted")]
        path: String,
    },
    #[command(about = "create credentials for a role")]
    Creds {
        #[arg(short, long, help = "role to generate credentials for")]
        name: String,
        #[arg(short, long, help = "path to the psql secrets engine mount")]
        path: String,
        #[arg(long, help = "time to live for credentials")]
        ttl: Option<humantime::Duration>,
    },
    #[command(about = "add a role")]
    AddRole {
        #[arg(short, long, help = "name of role to create")]
        name: String,
        #[arg(short, long)]
        path: String,
        #[arg(long)]
        sql: String,
        #[arg(long)]
        revocation_sql: String,
    },
}

impl Psql {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            PsqlSubcommand::SetConnection {
                connection_url,
                path,
            } => {
                let resp = sdk
                    .psql
                    .set_connection(
                        &path,
                        &SetConnectionParams {
                            connection_url,
                            verify_connection: true,
                            max_open_connections: None,
                        },
                    )
                    .await;
                handle_resp(resp);
            }
            PsqlSubcommand::Connection { path } => {
                let resp = sdk.psql.read_connection(&path).await;
                handle_resp(resp);
            }
            PsqlSubcommand::Creds { name, path, ttl } => {
                let ttl = ttl.map(|ttl| Duration::from_millis(ttl.as_millis() as u64));
                let resp = sdk.psql.create_credentials(&path, &name, ttl).await;
                handle_resp(resp);
            }
            PsqlSubcommand::AddRole {
                name,
                path,
                sql,
                revocation_sql,
            } => {
                let resp = sdk
                    .psql
                    .create_role(
                        &path,
                        &name,
                        &CreateRoleParams {
                            sql,
                            revocation_sql,
                        },
                    )
                    .await;
                handle_resp(resp);
            }
        }
    }
}
