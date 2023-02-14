use std::error::Error;

use clap::{Args, Subcommand};
use covert_sdk::{
    kv::{
        CreateSecretParams, HardDeleteSecretParams, RecoverSecretParams, SetConfigParams,
        SoftDeleteSecretParams,
    },
    Client,
};

use crate::handle_resp;

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
    Put {
        #[arg(help = "key to add secret to")]
        key: String,
        #[arg(short, long, value_parser = parse_key_val::<String, String>)]
        data: Vec<(String, String)>,
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "retrieve secret")]
    Get {
        #[arg(help = "key to retrieve")]
        key: String,
        #[arg(short, long)]
        path: String,
        #[arg(short, long)]
        version: Option<u32>,
    },
    #[command(about = "soft-delete secret, can be recovered with the \"recover\" subcommand")]
    Delete {
        #[arg(help = "key to delete")]
        key: String,
        #[arg(short, long, use_value_delimiter = true, value_delimiter = ',')]
        versions: Vec<u32>,
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "hard-delete secret, cannot be recovered")]
    HardDelete {
        #[arg(help = "key to delete")]
        key: String,
        #[arg(short, long, use_value_delimiter = true, value_delimiter = ',')]
        versions: Vec<u32>,
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "recover soft-deleted secret")]
    Recover {
        #[arg(help = "key to recover")]
        key: String,
        #[arg(short, long, use_value_delimiter = true, value_delimiter = ',')]
        versions: Vec<u32>,
        #[arg(short, long)]
        path: String,
    },
    #[command(about = "update config for the kv backend")]
    SetConfig {
        #[arg(help = "path where the KV backend is mounted")]
        path: String,
        #[arg(long)]
        max_versions: u32,
    },
    #[command(about = "read config for the kv backend")]
    Config {
        #[arg(help = "path where the KV backend is mounted")]
        path: String,
    },
}

impl Kv {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            KvSubcommand::Put { key, data, path } => {
                let resp = sdk
                    .kv
                    .create(
                        &path,
                        &key,
                        &CreateSecretParams {
                            data: data.into_iter().collect(),
                        },
                    )
                    .await;
                handle_resp(resp);
            }
            KvSubcommand::Get { key, path, version } => {
                let resp = sdk.kv.read(&path, &key, version).await;
                handle_resp(resp);
            }
            KvSubcommand::Recover {
                key,
                path,
                versions,
            } => {
                let resp = sdk
                    .kv
                    .recover(&path, &key, &RecoverSecretParams { versions })
                    .await;
                handle_resp(resp);
            }
            KvSubcommand::SetConfig { max_versions, path } => {
                let resp = sdk
                    .kv
                    .set_config(&path, &SetConfigParams { max_versions })
                    .await;
                handle_resp(resp);
            }
            KvSubcommand::Config { path } => {
                let resp = sdk.kv.read_config(&path).await;
                handle_resp(resp);
            }
            KvSubcommand::Delete {
                key,
                versions,
                path,
            } => {
                let resp = sdk
                    .kv
                    .delete(&path, &key, &SoftDeleteSecretParams { versions })
                    .await;
                handle_resp(resp);
            }
            KvSubcommand::HardDelete {
                key,
                versions,
                path,
            } => {
                let resp = sdk
                    .kv
                    .hard_delete(&path, &key, &HardDeleteSecretParams { versions })
                    .await;
                handle_resp(resp);
            }
        }
    }
}
