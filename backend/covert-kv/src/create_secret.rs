use std::sync::Arc;

use chrono::Utc;

use super::Context;
use crate::{
    domain::secret::Secret,
    error::{Error, ErrorType},
};
use covert_framework::extract::{Extension, Json, Path, Query};
use covert_types::{
    methods::kv::{CreateSecretParams, CreateSecretResponse, ReadSecretQuery, ReadSecretResponse},
    response::Response,
};

#[tracing::instrument(skip_all)]
pub async fn add_secret(
    Extension(ctx): Extension<Arc<Context>>,
    Path(key): Path<String>,
    Json(body): Json<CreateSecretParams>,
) -> Result<Response, Error> {
    let version_metadata = ctx.repos.secrets.version_metadata(&key).await?;

    let value = serde_json::to_string(&body.data)?;
    let secret = Secret {
        key: key.clone(),
        version: version_metadata.map_or(0, |v| v.max_version + 1),
        value: Some(value),
        created_time: Utc::now(),
        deleted: false,
        destroyed: false,
    };
    ctx.repos.secrets.insert(&secret).await?;

    let config = ctx.repos.config.load().await?;
    ctx.repos
        .secrets
        .prune_old_versions(&key, config.max_versions)
        .await?;

    let version_metadata = ctx
        .repos
        .secrets
        .version_metadata(&key)
        .await?
        .ok_or_else(|| {
            ErrorType::InternalError(anyhow::Error::msg(
                "Metadata for key should not be null when a new version has just been added",
            ))
        })?;

    let resp = CreateSecretResponse {
        version: secret.version,
        created_time: secret.created_time,
        deleted: secret.deleted,
        destroyed: secret.destroyed,
        min_version: version_metadata.min_version,
        max_version: version_metadata.max_version,
    };
    Response::raw(resp).map_err(Into::into)
}

#[tracing::instrument(skip_all)]
pub async fn read_secret(
    Extension(ctx): Extension<Arc<Context>>,
    Path(key): Path<String>,
    Query(query): Query<ReadSecretQuery>,
) -> Result<Response, Error> {
    let version_metadata = ctx
        .repos
        .secrets
        .version_metadata(&key)
        .await?
        .ok_or(ErrorType::MetadataNotFound)?;

    let version = query.version.unwrap_or(version_metadata.max_version);
    let secret = ctx
        .repos
        .secrets
        .get(&key, version)
        .await?
        .ok_or(ErrorType::KeyVersionNotFound)?;

    let mut resp = ReadSecretResponse {
        data: None,
        metadata: CreateSecretResponse {
            version,
            min_version: version_metadata.min_version,
            max_version: version_metadata.max_version,
            created_time: secret.created_time,
            deleted: secret.deleted,
            destroyed: secret.destroyed,
        },
    };

    if !secret.deleted && !secret.destroyed {
        resp.data = secret
            .value
            .as_ref()
            .map(|value| serde_json::from_str(value))
            .transpose()?;
    }

    Response::raw(resp).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bytes::Bytes;
    use covert_types::auth::AuthPolicy;
    use covert_types::methods::kv::SetConfigParams;
    use covert_types::request::{Operation, Request};
    use covert_types::state::VaultState;

    use super::*;
    use crate::config::tests::{setup, TestContext};
    use crate::domain::config::DEFAULT_MAX_VERSIONS;

    #[sqlx::test]
    async fn put() {
        let b = setup().await.backend;

        let data = vec![("bar".to_string(), "baz".to_string())]
            .into_iter()
            .collect::<HashMap<_, _>>();
        let data = CreateSecretParams { data };
        let data = serde_json::to_vec(&data).unwrap();
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: Operation::Create,
            path: "data/foo".into(),
            data: Bytes::from(data),
            extensions,
            token: None,
            is_sudo: true,
            params: Default::default(),
            query_string: Default::default(),
            headers: Default::default(),
        };
        let resp = b.handle_request(req).await;
        assert!(resp.is_ok());

        let resp_data = resp.unwrap().data::<CreateSecretResponse>().unwrap();
        assert_eq!(resp_data.version, 1);

        let data = CreateSecretParams {
            data: vec![("bar".to_string(), "baz1".to_string())]
                .into_iter()
                .collect(),
        };
        let data = serde_json::to_vec(&data).unwrap();
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: Operation::Create,
            path: "data/foo".into(),
            data: Bytes::from(data),
            extensions,
            token: None,
            is_sudo: true,
            params: Default::default(),
            query_string: Default::default(),
            headers: Default::default(),
        };
        let resp = b.handle_request(req).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().data::<CreateSecretResponse>().unwrap();

        assert_eq!(resp_data.version, 2);
    }

    #[sqlx::test]
    async fn get() {
        let b = setup().await.backend;

        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: Operation::Read,
            path: "data/foo".into(),
            data: Bytes::default(),
            extensions,
            token: None,
            is_sudo: true,
            params: Default::default(),
            query_string: Default::default(),
            headers: Default::default(),
        };
        let resp = b.handle_request(req).await;
        assert!(resp.is_err());

        let data = CreateSecretParams {
            data: vec![("bar".to_string(), "baz".to_string())]
                .into_iter()
                .collect(),
        };
        let data_raw = serde_json::to_vec(&data).unwrap();
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: Operation::Create,
            path: "data/foo".into(),
            data: Bytes::from(data_raw),
            extensions,
            token: None,
            is_sudo: true,
            params: Default::default(),
            query_string: Default::default(),
            headers: Default::default(),
        };
        let resp = b.handle_request(req).await;
        assert!(resp.is_ok());

        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: Operation::Read,
            path: "data/foo".into(),
            data: Bytes::default(),
            extensions,
            token: None,
            is_sudo: true,
            params: Default::default(),
            query_string: Default::default(),
            headers: Default::default(),
        };
        let resp = b.handle_request(req).await;
        let resp_data = resp.unwrap().data::<ReadSecretResponse>().unwrap();
        assert_eq!(resp_data.data, Some(data.data));
        assert_eq!(resp_data.metadata.version, 1);
        assert!(
            resp_data.metadata.created_time.timestamp_millis()
                > Utc::now().timestamp_millis() - 60 * 1000
        );
        assert!(resp_data.metadata.created_time <= Utc::now());
    }

    async fn cleanup_old_versions(op: Operation) {
        let TestContext { backend: b, ctx } = setup().await;

        // Write max versions
        for i in 0..DEFAULT_MAX_VERSIONS {
            let data = CreateSecretParams {
                data: vec![("bar".to_string(), "baz".to_string())]
                    .into_iter()
                    .collect(),
            };
            let data_raw = serde_json::to_vec(&data).unwrap();
            let mut extensions = http::Extensions::new();
            extensions.insert(AuthPolicy::Authenticated);
            extensions.insert(VaultState::Unsealed);
            let req = Request {
                id: Default::default(),
                operation: Operation::Create,
                path: "data/foo".into(),
                data: Bytes::from(data_raw),
                extensions,
                token: None,
                is_sudo: true,
                params: Default::default(),
                query_string: Default::default(),
                headers: Default::default(),
            };
            let resp = b.handle_request(req).await;
            assert!(resp.is_ok());
            let resp_data = resp.unwrap().data::<CreateSecretResponse>().unwrap();
            assert_eq!(resp_data.version, i + 1);
        }

        // lower max versions
        let data = SetConfigParams { max_versions: 2 };
        let data = serde_json::to_vec(&data).unwrap();
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: Operation::Update,
            path: "config".into(),
            data: Bytes::from(data),
            extensions,
            token: None,
            is_sudo: true,
            params: Default::default(),
            query_string: Default::default(),
            headers: Default::default(),
        };
        let resp = b.handle_request(req).await;
        assert!(resp.is_ok());

        // write another version
        let data = CreateSecretParams {
            data: vec![("bar".to_string(), "baz".to_string())]
                .into_iter()
                .collect(),
        };
        let data_raw = serde_json::to_vec(&data).unwrap();
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: op,
            path: "data/foo".into(),
            data: Bytes::from(data_raw),
            extensions,
            token: None,
            is_sudo: true,
            params: Default::default(),
            query_string: Default::default(),
            headers: Default::default(),
        };
        let resp = b.handle_request(req).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().data::<CreateSecretResponse>().unwrap();
        assert_eq!(resp_data.version, 11);

        // Make sure versions 1-9 were cleaned up.
        for i in 1..=9 {
            let v = ctx.repos.secrets.get("foo", i).await.unwrap();
            assert!(v.is_none());
        }
    }

    // TODO: inline the function call
    #[sqlx::test]
    async fn put_cleanup_old_versions() {
        cleanup_old_versions(Operation::Update).await;
    }
}
