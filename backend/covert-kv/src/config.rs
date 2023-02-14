use std::sync::Arc;

use covert_framework::extract::{Extension, Json};
use covert_types::{
    methods::kv::{ReadConfigResponse, SetConfigParams, SetConfigResponse},
    response::Response,
};

use crate::{domain::config::Configuration, error::Error};

use super::Context;

#[tracing::instrument(skip(ctx))]
pub async fn set_config(
    Extension(ctx): Extension<Arc<Context>>,
    Json(body): Json<SetConfigParams>,
) -> Result<Response, Error> {
    let config = Configuration {
        max_versions: body.max_versions,
    };
    ctx.repos.config.set(&config).await?;
    let resp = SetConfigResponse {
        max_versions: config.max_versions,
    };
    Response::raw(resp).map_err(Into::into)
}

#[tracing::instrument(skip_all)]
pub async fn read_config(Extension(ctx): Extension<Arc<Context>>) -> Result<Response, Error> {
    let config = ctx.repos.config.load().await?;

    let resp = ReadConfigResponse {
        max_versions: config.max_versions,
    };

    Response::raw(resp).map_err(Into::into)
}

#[cfg(test)]
pub(super) mod tests {
    use bytes::Bytes;
    use covert_framework::Backend;
    use covert_storage::{migrator::migrate_backend, BackendStoragePool, EncryptedPool};
    use covert_types::{
        auth::AuthPolicy,
        request::{Operation, Request},
        state::VaultState,
    };

    use crate::{new_versioned_kv_backend, Migrations};

    use super::*;

    pub struct TestContext {
        pub backend: Backend,
        pub ctx: Context,
    }

    pub async fn setup() -> TestContext {
        let pool = Arc::new(EncryptedPool::new_tmp());

        let storage = BackendStoragePool::new("foo_", pool);

        migrate_backend::<Migrations>(&storage).await.unwrap();

        TestContext {
            backend: new_versioned_kv_backend(storage.clone()).unwrap(),
            ctx: Context::new(storage),
        }
    }

    #[sqlx::test]
    async fn config() {
        let b = setup().await.backend;

        let data = SetConfigParams { max_versions: 12 };
        let data = serde_json::to_vec(&data).unwrap();
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: Operation::Create,
            path: "config".into(),
            data: Bytes::from(data),
            token: None,
            is_sudo: true,
            extensions,
            params: Default::default(),
            query_string: Default::default(),
            headers: Default::default(),
        };
        let resp = b.handle_request(req).await;
        assert!(resp.is_ok());

        // Read
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: Operation::Read,
            path: "config".into(),
            data: Bytes::default(),
            token: None,
            is_sudo: true,
            extensions,
            params: Default::default(),
            query_string: Default::default(),
            headers: Default::default(),
        };
        let resp = b.handle_request(req).await.unwrap();
        let data: ReadConfigResponse = resp.data().unwrap();
        assert_eq!(data.max_versions, 12);
    }
}
