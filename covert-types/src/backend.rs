use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

#[derive(
    Debug, Copy, Clone, PartialEq, EnumString, Display, SerializeDisplay, DeserializeFromStr, Eq,
)]
pub enum BackendType {
    #[strum(ascii_case_insensitive, serialize = "kv")]
    Kv,
    #[strum(ascii_case_insensitive, serialize = "postgres", serialize = "psql")]
    Postgres,
    #[strum(ascii_case_insensitive, serialize = "sys", serialize = "system")]
    System,
    #[strum(ascii_case_insensitive, serialize = "userpass")]
    Userpass,
}

#[derive(
    Debug, Copy, Clone, PartialEq, EnumString, Display, SerializeDisplay, DeserializeFromStr,
)]
pub enum BackendCategory {
    #[strum(ascii_case_insensitive, serialize = "secret")]
    Logical,
    #[strum(ascii_case_insensitive, serialize = "auth")]
    Credential,
}

impl From<BackendType> for BackendCategory {
    fn from(value: BackendType) -> Self {
        match value {
            BackendType::Kv | BackendType::Postgres | BackendType::System => {
                BackendCategory::Logical
            }
            BackendType::Userpass => BackendCategory::Credential,
        }
    }
}
