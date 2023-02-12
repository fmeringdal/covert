use std::str::FromStr;

use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;

const TOKEN_LENGTH: usize = 24;

enum TokenType {
    Service,
}

impl TokenType {
    pub fn prefix(&self) -> &'static str {
        match self {
            TokenType::Service => "hvs",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token(String);

impl FromStr for Token {
    type Err = ApiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with(TokenType::Service.prefix()) {
            // TODO: more validation of string
            Ok(Self(s.to_string()))
        } else {
            Err(ApiError::bad_request())
        }
    }
}

impl Token {
    #[must_use]
    pub fn new() -> Self {
        let mut rng = thread_rng();
        let chars: String = (0..TOKEN_LENGTH)
            .map(|_| rng.sample(Alphanumeric) as char)
            .collect();
        let token = format!("{}.{chars}", TokenType::Service.prefix());
        Self(token)
    }

    // Not using the ToString/Display trait to prevent accidental leaks
    #[allow(clippy::inherent_to_string)]
    #[must_use]
    pub fn to_string(&self) -> String {
        self.0.clone()
    }
}

impl Default for Token {
    fn default() -> Self {
        Self::new()
    }
}
