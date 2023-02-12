use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct Response<Data, Includes> {
    pub data: Data,
    pub includes: Option<Includes>,
    pub meta: Option<Meta>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub next_token: Option<String>,
    pub result_count: i64,
    pub newest_id: Option<String>,
    pub oldest_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub username: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tweet {
    pub id: String,
    pub text: String,
    pub created_at: String,
    pub author_id: String,
    pub author_username: Option<String>,
}
