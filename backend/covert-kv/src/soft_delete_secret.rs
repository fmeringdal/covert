use covert_framework::extract::{Extension, Json, Path};
use covert_types::{
    methods::kv::{
        RecoverSecretParams, RecoverSecretResponse, SoftDeleteSecretParams,
        SoftDeleteSecretResponse,
    },
    response::Response,
};

use std::sync::Arc;

use crate::error::{Error, ErrorType};

use super::Context;

#[tracing::instrument(skip_all)]
pub async fn soft_delete_secret(
    Extension(ctx): Extension<Arc<Context>>,
    Path(key): Path<String>,
    Json(body): Json<SoftDeleteSecretParams>,
) -> Result<Response, Error> {
    if body.versions.is_empty() {
        return Err(ErrorType::MissingKeyVersions.into());
    }

    let not_deleted = ctx.repos.secrets.soft_delete(&key, &body.versions).await?;

    let resp = SoftDeleteSecretResponse { not_deleted };
    Response::raw(resp).map_err(Into::into)
}

#[tracing::instrument(skip_all)]
pub async fn path_undelete_write(
    Extension(ctx): Extension<Arc<Context>>,
    Path(key): Path<String>,
    Json(body): Json<RecoverSecretParams>,
) -> Result<Response, Error> {
    if body.versions.is_empty() {
        return Err(ErrorType::MissingKeyVersions.into());
    }

    let not_recovered = ctx.repos.secrets.recover(&key, &body.versions).await?;

    let resp = RecoverSecretResponse { not_recovered };
    Response::raw(resp).map_err(Into::into)
}

#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use bytes::Bytes;
    use covert_types::auth::AuthPolicy;
    use covert_types::methods::kv::{
        CreateSecretParams, CreateSecretResponse, ReadSecretResponse, SoftDeleteSecretParams,
    };
    use covert_types::request::{Operation, Request};
    use covert_types::state::VaultState;

    use crate::config::tests::setup;

    #[sqlx::test]
    async fn delete() {
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
            data: Bytes::from(data.clone()),
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
            data: Bytes::from(data.clone()),
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

        let data = SoftDeleteSecretParams {
            versions: vec![1, 2],
        };
        let data = serde_json::to_vec(&data).unwrap();
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: Operation::Create,
            path: "delete/foo".into(),
            data: Bytes::from(data.clone()),
            extensions,
            token: None,
            is_sudo: true,
            params: Default::default(),
            query_string: Default::default(),
            headers: Default::default(),
        };
        let resp = b.handle_request(req).await;
        assert!(resp.is_ok());

        for version in [1, 2] {
            let mut extensions = http::Extensions::new();
            extensions.insert(AuthPolicy::Authenticated);
            extensions.insert(VaultState::Unsealed);
            let req = Request {
                id: Default::default(),
                operation: Operation::Read,
                path: "data/foo".into(),
                query_string: format!("version={version}"),
                data: Bytes::default(),
                extensions,
                token: None,
                is_sudo: true,
                params: Default::default(),
                headers: Default::default(),
            };
            let resp = b.handle_request(req).await;
            assert!(resp.is_ok());
            let resp_data = resp.unwrap().data::<ReadSecretResponse>().unwrap();
            assert!(resp_data.data.is_none());
            assert!(!resp_data.metadata.destroyed);
            assert!(resp_data.metadata.deleted);
            assert_eq!(resp_data.metadata.version, version);
        }
    }

    #[sqlx::test]
    async fn undelete() {
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

        let data = SoftDeleteSecretParams {
            versions: vec![1, 2],
        };
        let data = serde_json::to_vec(&data).unwrap();
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Authenticated);
        extensions.insert(VaultState::Unsealed);
        let req = Request {
            id: Default::default(),
            operation: Operation::Create,
            path: "delete/foo".into(),
            data: Bytes::from(data.clone()),
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
            operation: Operation::Create,
            path: "undelete/foo".into(),
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

        for version in [1, 2] {
            let mut extensions = http::Extensions::new();
            extensions.insert(AuthPolicy::Authenticated);
            extensions.insert(VaultState::Unsealed);
            let req = Request {
                id: Default::default(),
                operation: Operation::Read,
                path: "data/foo".into(),
                query_string: format!("version={version}"),
                data: Bytes::default(),
                extensions,
                token: None,
                is_sudo: true,
                params: Default::default(),
                headers: Default::default(),
            };
            let resp = b.handle_request(req).await;
            assert!(resp.is_ok());
            let resp_data = resp.unwrap().data::<ReadSecretResponse>().unwrap();
            assert!(resp_data.data.is_some());
            assert!(!resp_data.metadata.destroyed);
            assert!(!resp_data.metadata.deleted);
            assert_eq!(resp_data.metadata.version, version);
        }
    }
}
