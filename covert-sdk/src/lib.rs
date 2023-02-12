use std::sync::Arc;

use base::BaseClient;

pub(crate) mod base;
pub mod entity;
pub mod kv;
pub mod lease;
pub mod mounts;
pub mod operator;
pub mod policy;
pub mod psql;
pub mod status;
pub mod userpass;
pub(crate) mod utils;

pub struct Client {
    pub entity: crate::entity::Client,
    pub policy: crate::policy::Client,
    pub operator: crate::operator::Client,
    pub status: crate::status::Client,
    pub mount: crate::mounts::Client,
    pub kv: crate::kv::Client,
    pub psql: crate::psql::Client,
    pub userpass: crate::userpass::Client,
    pub lease: crate::lease::Client,
}

impl Client {
    pub fn new(api_url: impl ToString) -> Self {
        let base_client = Arc::new(BaseClient::new(api_url));

        let entity = crate::entity::Client::new(Arc::clone(&base_client));
        let policy = crate::policy::Client::new(Arc::clone(&base_client));
        let operator = crate::operator::Client::new(Arc::clone(&base_client));
        let status = crate::status::Client::new(Arc::clone(&base_client));
        let mounts = crate::mounts::Client::new(Arc::clone(&base_client));
        let kv = crate::kv::Client::new(Arc::clone(&base_client));
        let psql = crate::psql::Client::new(Arc::clone(&base_client));
        let userpass = crate::userpass::Client::new(Arc::clone(&base_client));
        let lease = crate::lease::Client::new(Arc::clone(&base_client));

        Self {
            entity,
            policy,
            operator,
            status,
            mount: mounts,
            kv,
            psql,
            userpass,
            lease,
        }
    }
}
