use reqwest::RequestBuilder;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Serialize, Deserialize)]
pub struct Response<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub(crate) struct BaseClient {
    api_url: String,
    token: RwLock<Option<String>>,
    namespace: RwLock<Option<String>>,
}

impl BaseClient {
    pub fn new(api_url: impl ToString) -> Self {
        let namespace = std::env::var("COVERT_NAMESPACE").ok();

        Self {
            api_url: api_url.to_string(),
            token: RwLock::new(None),
            namespace: RwLock::new(namespace),
        }
    }

    pub async fn set_token(&self, token: Option<String>) {
        let mut token_l = self.token.write().await;
        *token_l = token;
    }

    pub async fn set_namespace(&self, namespace: Option<String>) {
        let mut ns_l = self.namespace.write().await;
        *ns_l = namespace;
    }

    pub async fn send<T: for<'de> serde::de::Deserialize<'de>>(
        &self,
        mut rb: RequestBuilder,
    ) -> Result<T, String> {
        let token_l = self.token.read().await;
        if let Some(token) = token_l.as_ref() {
            rb = rb.header("X-Covert-Token", token);
        }
        drop(token_l);

        let ns_l = self.namespace.read().await;
        if let Some(ns) = ns_l.as_ref() {
            rb = rb.header("X-Covert-Namespace", ns);
        }
        drop(ns_l);

        rb.send()
            .await
            .map_err(|e| format!("{e:#?}"))?
            .json::<Response<T>>()
            .await
            .map_err(|e| format!("{e:#?}"))
            .and_then(|res| {
                if let Some(data) = res.data {
                    Ok(data)
                } else if let Some(err) = res.error {
                    Err(err)
                } else {
                    Err("Unexpected emtpy response from server".into())
                }
            })
    }

    pub async fn get<T: for<'de> serde::de::Deserialize<'de>>(
        &self,
        path: String,
    ) -> Result<T, String> {
        let client = reqwest::Client::new();
        let request_builder = client.get(format!("{}{}", self.api_url, path));
        self.send(request_builder).await
    }

    pub async fn delete<T: for<'de> serde::de::Deserialize<'de>>(
        &self,
        path: String,
    ) -> Result<T, String> {
        let client = reqwest::Client::new();
        let request_builder = client.delete(format!("{}{}", self.api_url, path));
        self.send(request_builder).await
    }

    pub async fn put<T: Serialize, U: for<'de> serde::de::Deserialize<'de>>(
        &self,
        path: String,
        body: &T,
    ) -> Result<U, String> {
        let client = reqwest::Client::new();
        let request_builder = client.put(format!("{}{}", self.api_url, path)).json(body);
        self.send(request_builder).await
    }

    pub async fn post<T: Serialize, U: for<'de> serde::de::Deserialize<'de>>(
        &self,
        path: String,
        body: &T,
    ) -> Result<U, String> {
        let client = reqwest::Client::new();
        let request_builder = client.post(format!("{}{}", self.api_url, path)).json(body);
        self.send(request_builder).await
    }
}
