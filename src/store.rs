use crate::twitter_client::{api, PagedResult, TwitterClient};
use crate::user_config::UserConfig;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;

// NB: all the synchronization and interior mutability are encapsulated here for granularity.
// Also it seems slightly nicer as an API?  Esp. since methods don't have to be &mut self.

#[derive(Debug)]
pub struct Store {
    pub twitter_client: TwitterClient,
    pub twitter_user: api::User,
    pub tweets: Arc<Mutex<HashMap<String, api::Tweet>>>,
    pub tweets_feed: Arc<Mutex<Vec<String>>>,
    pub tweets_feed_page_token: Arc<AsyncMutex<Option<String>>>,
    pub user_config: Arc<Mutex<UserConfig>>,
}

impl Store {
    pub fn new(
        twitter_client: TwitterClient,
        twitter_user: &api::User,
        user_config: &UserConfig,
    ) -> Self {
        Self {
            twitter_client,
            twitter_user: twitter_user.clone(),
            tweets: Arc::new(Mutex::new(HashMap::new())),
            tweets_feed: Arc::new(Mutex::new(Vec::new())),
            tweets_feed_page_token: Arc::new(AsyncMutex::new(None)),
            user_config: Arc::new(Mutex::new(user_config.clone())),
        }
    }

    pub fn save_user_config(&self) -> Result<()> {
        let user_config = self.user_config.lock().unwrap();
        let user_config = serde_json::to_string(&*user_config)?;
        fs::write("./var/.user_config", user_config)?;
        Ok(())
    }

    // pub async fn load_tweet(&self, tweet_id: &str) {}

    // CR: need to sift results
    // CR: need a fixed page size, then call the twitter_client as many times as needed to achieve
    // the desired page effect
    pub async fn load_tweets_feed<
        F: Future<Output = PagedResult<Vec<api::Tweet>>>,
        G: Fn(Option<String>) -> F,
    >(
        &self,
        g: G,
        restart: bool,
    ) -> Result<()> {
        let mut tweets_page_token = self
            .tweets_feed_page_token
            .try_lock()
            .with_context(|| anyhow!("Already in-flight"))?;

        let mut maybe_page_token = None;
        // NB: require page token if continuing to next page
        if !restart {
            let next_page_token = tweets_page_token.as_ref().ok_or(anyhow!("No more pages"))?;
            maybe_page_token = Some(next_page_token.clone());
        }

        let (new_tweets, page_token) = g(maybe_page_token).await?;
        let mut new_tweets_reverse_chronological: Vec<String> = Vec::new();

        *tweets_page_token = page_token;

        {
            let mut tweets = self.tweets.lock().unwrap();
            for tweet in new_tweets {
                new_tweets_reverse_chronological.push(tweet.id.clone());
                tweets.insert(tweet.id.clone(), tweet);
            }
        }
        {
            let mut tweets_reverse_chronological = self.tweets_feed.lock().unwrap();
            if restart {
                *tweets_reverse_chronological = new_tweets_reverse_chronological;
            } else {
                tweets_reverse_chronological.append(&mut new_tweets_reverse_chronological);
            }
        }

        Ok(())
    }

    pub async fn load_tweets_reverse_chronological(&self, restart: bool) -> Result<()> {
        self.load_tweets_feed(
            move |maybe_page_token| async move {
                self.twitter_client
                    .timeline_reverse_chronological(&self.twitter_user.id, maybe_page_token)
                    .await
            },
            restart,
        )
        .await
    }

    pub async fn load_user_tweets(&self, user_id: &str, restart: bool) -> Result<()> {
        self.load_tweets_feed(
            move |maybe_page_token| async move {
                self.twitter_client
                    .user_tweets(user_id, maybe_page_token)
                    .await
            },
            restart,
        )
        .await
    }
}
