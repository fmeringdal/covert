use std::time::Duration;

use clap::{Args, Subcommand};
use covert_sdk::Client;

use crate::handle_resp;

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
    Renew {
        lease_id: String,
        #[arg(long)]
        ttl: Option<humantime::Duration>,
    },
    #[command(about = "lookup lease")]
    Lookup { lease_id: String },
    #[command(about = "revoke leases by mount path prefix")]
    RevokeMount { prefix: String },
    #[command(about = "list leases by mount path prefix")]
    ListMount { prefix: String },
}

impl Leases {
    pub async fn handle(self, sdk: &Client) {
        match self.subcommand {
            LeasesSubcommand::Revoke { lease_id } => {
                let resp = sdk.lease.revoke(&lease_id).await;
                handle_resp(resp);
            }
            LeasesSubcommand::Renew { lease_id, ttl } => {
                let ttl = ttl.map(|ttl| Duration::from_millis(ttl.as_millis() as u64));
                let resp = sdk.lease.renew(&lease_id, ttl).await;
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
        }
    }
}
