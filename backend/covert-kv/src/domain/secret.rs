use chrono::{DateTime, Utc};

#[derive(Debug, sqlx::FromRow, PartialEq, Eq)]
pub struct Secret {
    pub key: String,
    pub version: u32,
    pub value: Option<String>,
    pub created_time: DateTime<Utc>,
    pub deleted: bool,
    pub destroyed: bool,
}
