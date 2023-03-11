use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response<Data, Includes> {
    pub data: Data,
    pub includes: Option<Includes>,
    pub meta: Option<Meta>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    pub next_token: Option<String>,
    pub result_count: i64,
    pub newest_id: Option<String>,
    pub oldest_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub username: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tweet {
    pub id: String,
    pub text: String,
    pub created_at: DateTime<Local>,
    pub author_id: String,
    pub author_username: Option<String>,
    pub author_name: Option<String>,
    pub conversation_id: Option<String>,
    pub referenced_tweets: Option<Vec<TweetReference>>,
    pub attachments: Option<Attachments>,
    pub public_metrics: Option<PublicMetrics>,
}

impl Tweet {
    pub fn author(&self, fill_unknown_with: &str) -> User {
        User {
            id: self.author_id.clone(),
            name: self
                .author_name
                .clone()
                .unwrap_or(fill_unknown_with.to_string()),
            username: self
                .author_username
                .clone()
                .unwrap_or(fill_unknown_with.to_string()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TweetReference {
    pub r#type: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attachments {
    pub poll_ids: Option<Vec<String>>,
    pub media_keys: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicMetrics {
    pub retweet_count: i32,
    pub reply_count: i32,
    pub like_count: i32,
    pub quote_count: i32,
}
