use std::{collections::HashMap, str::FromStr};

use bytes::Bytes;
use http::{Extensions, Method};
use http_body::Limited;
use hyper::Body;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth::AuthPolicy, error::ApiError, state::VaultState};

// Request is a struct that stores the parameters and context of a request
// being made to Vault. It is used to abstract the details of the higher level
// request protocol from the handlers.
//
// Note: Many of these have Sentinel disabled because they are values populated
// by the router after policy checks; the token namespace would be the right
// place to access them via Sentinel
#[derive(Debug)]
pub struct Request {
    // Id is the uuid associated with each request
    pub id: Uuid,

    // Operation is the requested operation type
    pub operation: Operation,

    pub path: String,

    pub data: Bytes,
    pub query_string: String,
    // TODO: don't use this
    pub extensions: http::Extensions,
    pub params: Vec<String>,
    pub token: Option<String>,
    pub is_sudo: bool,

    // Secret will be non-nil only for Revoke and Renew operations
    // to represent the secret that was returned prior.
    // Secret *Secret `json:"secret" structs:"secret" mapstructure:"secret" sentinel:""`

    // Auth will be non-nil only for Renew operations
    // to represent the auth that was returned prior.
    // Auth *Auth `json:"auth" structs:"auth" mapstructure:"auth" sentinel:""`

    // Headers will contain the http headers from the request. This value will
    // be used in the audit broker to ensure we are auditing only the allowed
    // headers.
    pub headers: HashMap<String, String>,
    // Headers map[string][]string `json:"headers" structs:"headers" mapstructure:"headers" sentinel:""`

    // Connection will be non-nil only for credential providers to
    // inspect the connection information and potentially use it for
    // authentication/protection.
    // Connection *Connection `json:"connection" structs:"connection" mapstructure:"connection"`

    // ClientToken is provided to the core so that the identity
    // can be verified and ACLs applied. This value is passed
    // through to the logical backends but after being salted and
    // hashed.
    // ClientToken string `json:"client_token" structs:"client_token" mapstructure:"client_token" sentinel:""`

    // ClientTokenAccessor is provided to the core so that the it can get
    // logged as part of request audit logging.
    // ClientTokenAccessor string `json:"client_token_accessor" structs:"client_token_accessor" mapstructure:"client_token_accessor" sentinel:""`

    // DisplayName is provided to the logical backend to help associate
    // dynamic secrets with the source entity. This is not a sensitive
    // name, but is useful for operators.
    // DisplayName string `json:"display_name" structs:"display_name" mapstructure:"display_name" sentinel:""`

    // MountPoint is provided so that a logical backend can generate
    // paths relative to itself. The `Path` is effectively the client
    // request path with the MountPoint trimmed off.
    // MountPoint string `json:"mount_point" structs:"mount_point" mapstructure:"mount_point" sentinel:""`

    // MountType is provided so that a logical backend can make decisions
    // based on the specific mount type (e.g., if a mount type has different
    // aliases, generating different defaults depending on the alias)
    // MountType string `json:"mount_type" structs:"mount_type" mapstructure:"mount_type" sentinel:""`

    // MountAccessor is provided so that identities returned by the authentication
    // backends can be tied to the mount it belongs to.
    // MountAccessor string `json:"mount_accessor" structs:"mount_accessor" mapstructure:"mount_accessor" sentinel:""`

    // WrapInfo contains requested response wrapping parameters
    // WrapInfo *RequestWrapInfo `json:"wrap_info" structs:"wrap_info" mapstructure:"wrap_info" sentinel:""`

    // ClientTokenRemainingUses represents the allowed number of uses left on the
    // token supplied
    // ClientTokenRemainingUses int `json:"client_token_remaining_uses" structs:"client_token_remaining_uses" mapstructure:"client_token_remaining_uses"`

    // EntityID is the identity of the caller extracted out of the token used
    // to make this request
    // EntityID string `json:"entity_id" structs:"entity_id" mapstructure:"entity_id" sentinel:""`

    // PolicyOverride indicates that the requestor wishes to override
    // soft-mandatory Sentinel policies
    // PolicyOverride bool `json:"policy_override" structs:"policy_override" mapstructure:"policy_override"`

    // Whether the request is unauthenticated, as in, had no client token
    // attached. Useful in some situations where the client token is not made
    // accessible.
    // Unauthenticated bool `json:"unauthenticated" structs:"unauthenticated" mapstructure:"unauthenticated"`

    // MFACreds holds the parsed MFA information supplied over the API as part of
    // X-Vault-MFA header
    // MFACreds MFACreds `json:"mfa_creds" structs:"mfa_creds" mapstructure:"mfa_creds" sentinel:""`

    // Cached token entry. This avoids another lookup in request handling when
    // we've already looked it up at http handling time. Note that this token
    // has not been "used", as in it will not properly take into account use
    // count limitations. As a result this field should only ever be used for
    // transport to a function that would otherwise do a lookup and then
    // properly use the token.
    // tokenEntry *TokenEntry

    // For replication, contains the last WAL on the remote side after handling
    // the request, used for best-effort avoidance of stale read-after-write
    // lastRemoteWAL uint64

    // ControlGroup holds the authorizations that have happened on this
    // request
    // ControlGroup *ControlGroup `json:"control_group" structs:"control_group" mapstructure:"control_group" sentinel:""`

    // ClientTokenSource tells us where the client token was sourced from, so
    // we can delete it before sending off to plugins
    // ClientTokenSource ClientTokenSource

    // HTTPRequest, if set, can be used to access fields from the HTTP request
    // that generated this logical.Request object, such as the request body.
    // HTTPRequest *http.Request `json:"-" sentinel:""`

    // ResponseWriter if set can be used to stream a response value to the http
    // request that generated this logical.Request object.
    // ResponseWriter *HTTPResponseWriter `json:"-" sentinel:""`

    // requiredState is used internally to propagate the X-Vault-Index request
    // header to later levels of request processing that operate only on
    // logical.Request.
    // requiredState []string

    // responseState is used internally to propagate the state that should appear
    // in response headers; it's attached to the request rather than the response
    // because not all requests yields non-nil responses.
    // responseState *WALState
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
            .get("X-Vault-Token")
            .map(|val| val.to_str().unwrap_or_default())
            .and_then(|token| {
                if token.is_empty() {
                    None
                } else {
                    Some(token.to_string())
                }
            });
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
            query_string: uri.query().unwrap_or_default().to_string(),
            path: path.to_string(),
            extensions: Extensions::new(),
            // http requests are only sudo if the token is sudo
            is_sudo: false,
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

    // Builder methods
    #[must_use]
    pub fn sudo() -> Self {
        let mut extensions = http::Extensions::new();
        extensions.insert(AuthPolicy::Root);
        extensions.insert(VaultState::Unsealed);

        Self {
            id: Uuid::default(),
            operation: Operation::Read,
            path: String::default(),
            data: Bytes::default(),
            query_string: String::default(),
            extensions,
            params: Vec::default(),
            token: None,
            is_sudo: true,
            headers: HashMap::default(),
        }
    }

    #[must_use]
    pub fn with_operation(mut self, operation: Operation) -> Self {
        self.operation = operation;
        self
    }

    #[must_use]
    pub fn with_path(mut self, path: &impl ToString) -> Self {
        self.path = path.to_string();
        self
    }
}
