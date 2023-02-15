use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub created_at: DateTime<Utc>,
    pub author_id: String,
    pub author_username: Option<String>,
    pub author_name: Option<String>,
    pub conversation_id: Option<String>,
    pub referenced_tweets: Option<Vec<TweetReference>>,
    pub attachments: Option<Attachments>,
    pub public_metrics: Option<PublicMetrics>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TweetReference {
    pub r#type: String,
    pub id: String
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attachments {
    pub poll_ids: Option<Vec<String>>,
    pub media_keys: Option<Vec<String>>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicMetrics {
    pub retweet_count: i32,
    pub reply_count: i32,
    pub like_count: i32,
    pub quote_count: i32
}
