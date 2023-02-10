use serde::{Deserialize, Serialize};
use serde_json::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct TwitterResponse<Data> {
    pub data: Data,
    pub meta: Option<TwitterMeta>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TwitterMeta {
    pub next_token: Option<String>,
    pub result_count: i64,
    pub newest_id: Option<String>,
    pub oldest_id: Option<String>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TwitterUser {
    pub id: String,
    pub name: String,
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tweet {
    pub id: String,
    pub text: String,
    pub created_at: String,
}
