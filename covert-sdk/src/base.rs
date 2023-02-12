use reqwest::RequestBuilder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Response<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub(crate) struct BaseClient {
    api_url: String,
    token: Option<String>,
}

impl BaseClient {
    pub fn new(api_url: impl ToString) -> Self {
        let url = api_url.to_string();
        if url.ends_with('/') {
            todo!()
        }
        Self {
            api_url: api_url.to_string(),
            token: None,
        }
    }

    pub async fn send<T: for<'de> serde::de::Deserialize<'de>>(
        rb: RequestBuilder,
    ) -> Result<T, String> {
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
        let mut request_builder = client.get(format!("{}{}", self.api_url, path));
        if let Some(token) = self.token.as_ref() {
            request_builder = request_builder.header("X-Vault-Token", token);
        }
        Self::send(request_builder).await
    }

    pub async fn delete<T: for<'de> serde::de::Deserialize<'de>>(
        &self,
        path: String,
    ) -> Result<T, String> {
        let client = reqwest::Client::new();
        let mut request_builder = client.delete(format!("{}{}", self.api_url, path));
        if let Some(token) = self.token.as_ref() {
            request_builder = request_builder.header("X-Vault-Token", token);
        }
        Self::send(request_builder).await
    }

    pub async fn put<T: Serialize, U: for<'de> serde::de::Deserialize<'de>>(
        &self,
        path: String,
        body: &T,
    ) -> Result<U, String> {
        let client = reqwest::Client::new();
        let mut request_builder = client.put(format!("{}{}", self.api_url, path)).json(body);
        if let Some(token) = self.token.as_ref() {
            request_builder = request_builder.header("X-Vault-Token", token);
        }
        Self::send(request_builder).await
    }

    pub async fn post<T: Serialize, U: for<'de> serde::de::Deserialize<'de>>(
        &self,
        path: String,
        body: &T,
    ) -> Result<U, String> {
        let client = reqwest::Client::new();
        let mut request_builder = client.post(format!("{}{}", self.api_url, path)).json(body);
        if let Some(token) = self.token.as_ref() {
            request_builder = request_builder.header("X-Vault-Token", token);
        }
        Self::send(request_builder).await
    }
}
