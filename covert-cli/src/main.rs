//! Covert command-line interface
//!
//! ```
//! covert app add kv secret
//! covert app add psql main-db-cluster
//! covert app config main-db-cluster -default-lease-time 15m
//! covert kv add -key stripe-secret-key -value 11jpojopfjpsoa2
//! covert kv remove -key stripe-secret-key
//! covert kv recover -key stripe-secret-key
//! covert help
//! covert status
//! covert operator init
//! covert operator unseal --unseal-keys "124214,125215,124521"
//! covert operator seal
//! covert policy add -name admin -policy '{ ... }'
//! covert entity add -name admin
//! covert entity add-policy admin
//! covert entity add-alias -alias admin-alias -mount-id 'asfasfa'
//! covert lease revoke 'asfsaojgopaj'
//! covert login userpass --username admin --password admin --path userpass
//! covert login token --token 124152151
//! covert server
//! cargo run policy add --name freddy-aws --policy "path \"sys/*\" { capabilities = [\"read\",\"update\",\"create\"] } path \"sys/v2\" { capabilities = [\"update\"] }"
//! ```
//!
//! KV example
//! ```
//! covert secrets enable -n kv -p kv/
//! covert secrets list
//! covert kv add -k fredrik -m kv/ -d "help=true" -d "ok=true"
//! covert kv read -k fredrik -m kv/
//! ```
//!
//! Userpass example
//! ```
//! covert auth enable -n userpass -p auth/userpass/
//! covert auth list
//! ```

use std::{error::Error, str::FromStr};

use clap::{arg, command, Args, Parser, Subcommand};
use covert_sdk::{
    entity::{
        AttachEntityAliasParams, AttachEntityPolicyParams, CreateEntityParams, EntityAlias,
        RemoveEntityAliasParams, RemoveEntityPolicyParams,
    },
    kv::{
        CreateSecretParams, HardDeleteSecretParams, RecoverSecretParams, SetConfigParams,
        SoftDeleteSecretParams,
    },
    mounts::{BackendType, CreateMountParams},
    operator::{InitializeParams, UnsealParams},
    policy::CreatePolicyParams,
    psql::{CreateRoleParams, SetConnectionParams},
    userpass::{CreateUserParams, UpdateUserPasswordParams},
    Client,
};
use serde::Serialize;

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

#[derive(Args, Debug)]
pub struct Server {
    #[arg(short, long, default_value_t = 8080, env = "COVERT_PORT")]
    port: u16,
    #[arg(short, long, env = "COVERT_STORAGE_PATH")]
    storage_path: Option<String>,
}

#[derive(Args, Debug)]
pub struct Entity {
    #[clap(subcommand)]
    subcommand: EntitySubcommand,
}

#[derive(Subcommand, Debug)]
pub enum EntitySubcommand {
    #[command(about = "add new entity")]
    Add {
        #[arg(short, long)]
        name: String,
    },
    #[command(about = "attach policy to entity")]
    AttachPolicy {
        #[arg(short, long)]
        name: String,
        #[arg(short, long, use_value_delimiter = true, value_delimiter = ',')]
        policies: Vec<String>,
    },
    #[command(about = "remove policy from entity")]
    RemovePolicy {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        policy: String,
    },
    #[command(about = "attach alias to entity")]
    AttachAlias {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        alias: String,
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "remove alias from entity")]
    RemoveAlias {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        alias: String,
        #[arg(short, long)]
        mount: String,
    },
}

#[derive(Args, Debug)]
pub struct Kv {
    #[clap(subcommand)]
    subcommand: KvSubcommand,
}

/// Parse a single key-value pair
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid key=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

#[derive(Subcommand, Debug)]
pub enum KvSubcommand {
    #[command(about = "add new secret")]
    Add {
        #[arg(short, long)]
        key: String,
        #[arg(short, long, value_parser = parse_key_val::<String, String>)]
        data: Vec<(String, String)>,
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "read secret")]
    Read {
        #[arg(short, long)]
        key: String,
        #[arg(short, long)]
        mount: String,
        #[arg(short, long)]
        version: Option<u32>,
    },
    #[command(about = "soft-delete secret, can be recovered with the \"recover\" subcommand")]
    Delete {
        #[arg(short, long)]
        key: String,
        #[arg(short, long, use_value_delimiter = true, value_delimiter = ',')]
        versions: Vec<u32>,
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "hard-delete secret, cannot be recovered")]
    HardDelete {
        #[arg(short, long)]
        key: String,
        #[arg(short, long, use_value_delimiter = true, value_delimiter = ',')]
        versions: Vec<u32>,
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "recover soft-deleted secret")]
    Recover {
        #[arg(short, long)]
        key: String,
        #[arg(short, long, use_value_delimiter = true, value_delimiter = ',')]
        versions: Vec<u32>,
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "update config for the kv backend")]
    SetConfig {
        #[arg(long)]
        max_versions: u32,
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "read config for the kv backend")]
    Config {
        #[arg(short, long)]
        mount: String,
    },
}

#[derive(Args, Debug)]
pub struct Auth {
    #[clap(subcommand)]
    subcommand: AuthSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum AuthSubcommand {
    #[command(about = "enable auth method")]
    Enable {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "disable auth method")]
    Disable {
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "list auth methods")]
    List,
}

#[derive(Args, Debug)]
pub struct Secrets {
    #[clap(subcommand)]
    subcommand: SecretsSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum SecretsSubcommand {
    #[command(about = "enable secret engine")]
    Enable {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "disable secret engine")]
    Disable {
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "list secret engines")]
    List,
}

#[derive(Args, Debug)]
pub struct Leases {
    #[clap(subcommand)]
    subcommand: LeasesSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum LeasesSubcommand {
    #[command(about = "revoke lease")]
    Revoke { lease_id: String },
    #[command(about = "renew lease")]
    Renew { lease_id: String },
    #[command(about = "lookup lease")]
    Lookup { lease_id: String },
    #[command(about = "revoke leases by mount path prefix")]
    RevokeMount { prefix: String },
    #[command(about = "list leases by mount path prefix")]
    ListMount { prefix: String },
}

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

#[derive(Args, Debug)]
pub struct Policy {
    #[clap(subcommand)]
    subcommand: PolicySubcommands,
}

#[derive(Subcommand, Debug)]
pub enum PolicySubcommands {
    #[command(about = "add new policy")]
    Add {
        #[arg(long)]
        policy: String,
        #[arg(long)]
        name: String,
    },
    #[command(about = "remove policy")]
    Remove {
        #[arg(long)]
        name: String,
    },
    #[command(about = "list policies")]
    List,
}

#[derive(Args, Debug)]
pub struct Psql {
    #[clap(subcommand)]
    subcommand: PsqlSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum PsqlSubcommand {
    #[command(about = "set connection config")]
    SetConnection {
        #[arg(long)]
        connection_url: String,
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "read connection config")]
    Connection {
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "create credentials for a role")]
    Creds {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "add a role")]
    AddRole {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        mount: String,
        #[arg(long)]
        sql: String,
        #[arg(long)]
        revocation_sql: String,
    },
}

#[derive(Args, Debug)]
pub struct Userpass {
    #[clap(subcommand)]
    subcommand: UserpassSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum UserpassSubcommand {
    #[command(about = "add user")]
    Add {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "remove user")]
    Remove {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "list users")]
    List {
        #[arg(short, long)]
        mount: String,
    },
    #[command(about = "update password for user")]
    UpdatePassword {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
        #[arg(long)]
        new_password: String,
        #[arg(short, long)]
        mount: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let sdk = Client::new(cli.covert_addr.clone());

    match cli.command {
        Commands::Entity(entity) => match entity.subcommand {
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
            EntitySubcommand::AttachAlias { name, alias, mount } => {
                let resp = sdk
                    .entity
                    .attach_alias(&AttachEntityAliasParams {
                        name,
                        aliases: vec![EntityAlias {
                            name: alias,
                            mount_path: mount,
                        }],
                    })
                    .await;
                handle_resp(resp);
            }
            EntitySubcommand::RemoveAlias { name, mount, alias } => {
                let resp = sdk
                    .entity
                    .remove_alias(
                        &name,
                        &RemoveEntityAliasParams {
                            alias: EntityAlias {
                                name: alias,
                                mount_path: mount,
                            },
                        },
                    )
                    .await;
                handle_resp(resp);
            }
        },
        Commands::Policy(policy) => match policy.subcommand {
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
        },
        Commands::Server(server) => match server.storage_path {
            Some(storage_path) => {
                let config = covert_server::Config {
                    port: server.port,
                    storage_path,
                };

                covert_server::start(config).await.unwrap()
            }
            None => {
                // TODO: auto unseal
                println!("Starting in dev mode. All data will be erased on exit.");
                let tmpdir = tempfile::tempdir().unwrap();
                let storage_path = tmpdir
                    .path()
                    .join("db-storage")
                    .to_str()
                    .unwrap()
                    .to_string();

                let config = covert_server::Config {
                    port: server.port,
                    storage_path,
                };

                covert_server::start(config).await.unwrap()
            }
        },
        Commands::Operator(operator) => match operator.subcommand {
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
        },
        Commands::Status => {
            let resp = sdk.status.status().await;
            handle_resp(resp);
        }
        Commands::Auth(auth) => match auth.subcommand {
            AuthSubcommand::Enable { name, path } => {
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
        },
        Commands::Secrets(secret) => match secret.subcommand {
            SecretsSubcommand::Enable { name, path } => {
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
        },
        Commands::Kv(kv) => match kv.subcommand {
            KvSubcommand::Add { key, data, mount } => {
                let resp = sdk
                    .kv
                    .create(
                        &mount,
                        &key,
                        &CreateSecretParams {
                            data: data.into_iter().collect(),
                        },
                    )
                    .await;
                handle_resp(resp);
            }
            KvSubcommand::Read {
                key,
                mount,
                version,
            } => {
                let resp = sdk.kv.read(&mount, &key, version).await;
                handle_resp(resp);
            }
            KvSubcommand::Recover {
                key,
                mount,
                versions,
            } => {
                let resp = sdk
                    .kv
                    .recover(&mount, &key, &RecoverSecretParams { versions })
                    .await;
                handle_resp(resp);
            }
            KvSubcommand::SetConfig {
                max_versions,
                mount,
            } => {
                let resp = sdk
                    .kv
                    .set_config(&mount, &SetConfigParams { max_versions })
                    .await;
                handle_resp(resp);
            }
            KvSubcommand::Config { mount } => {
                let resp = sdk.kv.read_config(&mount).await;
                handle_resp(resp);
            }
            KvSubcommand::Delete {
                key,
                versions,
                mount,
            } => {
                let resp = sdk
                    .kv
                    .delete(&mount, &key, &SoftDeleteSecretParams { versions })
                    .await;
                handle_resp(resp);
            }
            KvSubcommand::HardDelete {
                key,
                versions,
                mount,
            } => {
                let resp = sdk
                    .kv
                    .hard_delete(&mount, &key, &HardDeleteSecretParams { versions })
                    .await;
                handle_resp(resp);
            }
        },
        Commands::Psql(psql) => match psql.subcommand {
            PsqlSubcommand::SetConnection {
                connection_url,
                mount,
            } => {
                let resp = sdk
                    .psql
                    .set_connection(
                        &mount,
                        &SetConnectionParams {
                            connection_url,
                            verify_connection: true,
                            max_open_connections: None,
                        },
                    )
                    .await;
                handle_resp(resp);
            }
            PsqlSubcommand::Connection { mount } => {
                let resp = sdk.psql.read_connection(&mount).await;
                handle_resp(resp);
            }
            PsqlSubcommand::Creds { name, mount } => {
                let resp = sdk.psql.create_credentials(&mount, &name).await;
                handle_resp(resp);
            }
            PsqlSubcommand::AddRole {
                name,
                mount,
                sql,
                revocation_sql,
            } => {
                let resp = sdk
                    .psql
                    .create_role(
                        &mount,
                        &name,
                        &CreateRoleParams {
                            sql,
                            revocation_sql,
                        },
                    )
                    .await;
                handle_resp(resp);
            }
        },
        Commands::Userpass(userpass) => match userpass.subcommand {
            UserpassSubcommand::Add {
                username,
                password,
                mount,
            } => {
                let resp = sdk
                    .userpass
                    .create(&mount, &CreateUserParams { username, password })
                    .await;
                handle_resp(resp);
            }
            UserpassSubcommand::List { mount } => {
                let resp = sdk.userpass.list(&mount).await;
                handle_resp(resp);
            }
            UserpassSubcommand::Remove { mount, username } => {
                let resp = sdk.userpass.remove(&mount, &username).await;
                handle_resp(resp);
            }
            UserpassSubcommand::UpdatePassword {
                mount,
                username,
                password,
                new_password,
            } => {
                let resp = sdk
                    .userpass
                    .update_password(
                        &mount,
                        &username,
                        &UpdateUserPasswordParams {
                            password,
                            new_password,
                        },
                    )
                    .await;
                handle_resp(resp);
            }
        },
        Commands::Lease(lease) => match lease.subcommand {
            LeasesSubcommand::Revoke { lease_id } => {
                let resp = sdk.lease.revoke(&lease_id).await;
                handle_resp(resp);
            }
            LeasesSubcommand::Renew { lease_id } => {
                let resp = sdk.lease.renew(&lease_id).await;
                handle_resp(resp);
            }
            LeasesSubcommand::Lookup { lease_id } => {
                let resp = sdk.lease.lookup(&lease_id).await;
                handle_resp(resp);
            }
            LeasesSubcommand::ListMount { prefix } => {
                let resp = sdk.lease.list_by_mount(&prefix).await;
                handle_resp(resp);
            }
            LeasesSubcommand::RevokeMount { prefix } => {
                let resp = sdk.lease.revoke_by_mount(&prefix).await;
                handle_resp(resp);
            }
        },
    }
}

fn handle_resp<T: Serialize>(resp: Result<T, String>) {
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
