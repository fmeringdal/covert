//! Covert command-line interface

mod auth;
mod entity;
mod kv;
mod lease;
mod operator;
mod policy;
mod psql;
mod secrets;
mod server;
mod status;
mod userpass;

use auth::Auth;
use clap::{arg, command, Parser, Subcommand};
use covert_sdk::Client;
use entity::Entity;
use kv::Kv;
use lease::Leases;
use operator::Operator;
use policy::Policy;
use psql::Psql;
use secrets::Secrets;
use serde::Serialize;
use server::Server;
use status::handle_status;
use userpass::Userpass;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(long, env = "COVERT_ADDR", default_value = "http://127.0.0.1:8080/v1")]
    covert_addr: String,

    #[arg(long, env = "COVERT_TOKEN")]
    covert_token: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "check status")]
    Status,
    #[command(about = "useful subcommands for operators, typically used to initialize and unseal")]
    Operator(Operator),
    #[command(about = "manage entities")]
    Entity(Entity),
    #[command(about = "manage policies")]
    Policy(Policy),
    #[command(about = "manage auth methods")]
    Auth(Auth),
    #[command(about = "manage secret engines")]
    Secrets(Secrets),
    #[command(about = "start a Covert server")]
    Server(Server),
    #[command(about = "interact with a KV secrets engine")]
    Kv(Kv),
    #[command(about = "interact with a PostgreSQL secrets engine")]
    Psql(Psql),
    #[command(about = "interact with the userpass auth method")]
    Userpass(Userpass),
    #[command(about = "manage leases")]
    Lease(Leases),
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let sdk = Client::new(cli.covert_addr.clone());
    sdk.set_token(cli.covert_token).await;

    match cli.command {
        Commands::Entity(entity) => entity.handle(&sdk).await,
        Commands::Policy(policy) => policy.handle(&sdk).await,
        Commands::Server(server) => server.handle().await,
        Commands::Operator(operator) => operator.handle(&sdk).await,
        Commands::Status => handle_status(&sdk).await,
        Commands::Auth(auth) => auth.handle(&sdk).await,
        Commands::Secrets(secret) => secret.handle(&sdk).await,
        Commands::Kv(kv) => kv.handle(&sdk).await,
        Commands::Psql(psql) => psql.handle(&sdk).await,
        Commands::Userpass(userpass) => userpass.handle(&sdk).await,
        Commands::Lease(lease) => lease.handle(&sdk).await,
    }
}

pub(crate) fn handle_resp<T: Serialize>(resp: Result<T, String>) {
    match resp {
        Ok(resp) => {
            let resp = serde_json::to_string_pretty(&resp).unwrap();
            println!("{resp}");
        }
        Err(e) => {
            println!("Error: {e}");
        }
    }
}
