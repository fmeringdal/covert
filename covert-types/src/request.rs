use std::{collections::HashMap, str::FromStr};

use bytes::Bytes;
use http::{Extensions, Method};
use http_body::Limited;
use hyper::Body;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;

#[derive(Debug)]
pub struct Request {
    pub id: Uuid,

    pub operation: Operation,

    pub path: String,

    pub namespace: Vec<String>,

    pub data: Bytes,
    pub query_string: String,
    // TODO: don't use this
    pub extensions: http::Extensions,
    pub params: Vec<String>,
    pub token: Option<String>,

    pub headers: HashMap<String, String>,
}

/// Operation is an enum that is used to specify the type
/// of request being made
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operation {
    // The operations below are called per path
    Create,
    Read,
    Update,
    Delete,
    // The operations below are called globally, the path is less relevant.
    Revoke,
    Renew,
}

impl FromStr for Operation {
    type Err = ApiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &s.to_lowercase()[..] {
            "create" => Ok(Self::Create),
            "read" => Ok(Self::Read),
            "update" => Ok(Self::Update),
            "delete" => Ok(Self::Delete),
            "revoke" => Ok(Self::Revoke),
            "renew" => Ok(Self::Renew),
            _ => Err(ApiError::bad_request()),
        }
    }
}

impl Request {
    /// Create a internal logical request from a http request.
    ///
    /// # Errors
    ///
    /// Returns an error if the http request contains unsupported elements that
    /// cannot be converted to the logical request format.
    pub async fn new(raw: hyper::Request<Limited<Body>>) -> Result<Self, ApiError> {
        let uri = raw.uri().clone();
        let token = raw
            .headers()
            .get("X-Covert-Token")
            .map(|val| val.to_str().unwrap_or_default())
            .and_then(|token| {
                if token.is_empty() {
                    None
                } else {
                    Some(token.to_string())
                }
            });
        let namespace = raw
            .headers()
            .get("X-Covert-Namespace")
            .and_then(|namespace| namespace.to_str().ok())
            .map_or_else(
                || vec!["root".to_string()],
                |namespace| {
                    namespace
                        .trim()
                        .to_lowercase()
                        .split('/')
                        .map(ToString::to_string)
                        .filter(|ns| !ns.is_empty())
                        .collect::<Vec<_>>()
                },
            );
        let headers = raw
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();

        let operation = match *raw.method() {
            Method::GET => Operation::Read,
            Method::POST => Operation::Create,
            Method::PUT => Operation::Update,
            Method::DELETE => Operation::Delete,
            _ => return Err(ApiError::bad_request()),
        };

        let bytes = hyper::body::to_bytes(raw.into_body())
            .await
            .map_err(|_| ApiError::bad_request())?;

        let mut path = uri.path();
        if path.starts_with("/v1/") {
            path = &path[4..];
        }

        Ok(Self {
            id: Uuid::new_v4(),
            operation,
            namespace,
            query_string: uri.query().unwrap_or_default().to_string(),
            path: path.to_string(),
            extensions: Extensions::new(),
            token,
            params: vec![],
            data: bytes,
            headers,
        })
    }

    pub fn operation(&self) -> Operation {
        self.operation
    }

    pub fn advance_path(&mut self, prefix: &str) -> bool {
        if !self.path.starts_with(prefix) {
            return false;
        }

        self.path = self.path[prefix.len()..].to_string();

        true
    }
}
