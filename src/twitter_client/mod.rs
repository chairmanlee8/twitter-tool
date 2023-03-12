pub mod api;

use anyhow::{anyhow, Result};
use hyper::body::Bytes;
use hyper::client::HttpConnector;
use hyper::server::conn::Http;
use hyper::{Body, Client, Method, Request, Uri};
use hyper_tls::HttpsConnector;
use oauth2::basic::{BasicClient, BasicTokenResponse};
use oauth2::reqwest::async_http_client;
use oauth2::{
    AccessToken, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::{fs, process};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use url::Url;

pub type PagedResult<T> = Result<(T, Option<String>)>;

#[derive(Debug, Clone)]
pub struct TwitterClient {
    https_client: Client<HttpsConnector<HttpConnector>>,
    twitter_client_id: String,
    twitter_client_secret: String,
    twitter_auth: TwitterAuth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TwitterAuth {
    access_token: Option<AccessToken>,
    refresh_token: Option<RefreshToken>,
}

impl TwitterClient {
    pub fn new(twitter_client_id: &str, twitter_client_secret: &str) -> Self {
        let https = HttpsConnector::new();
        let https_client = Client::builder().build::<_, hyper::Body>(https);
        Self {
            https_client,
            twitter_client_id: twitter_client_id.to_string(),
            twitter_client_secret: twitter_client_secret.to_string(),
            twitter_auth: TwitterAuth {
                access_token: None,
                refresh_token: None,
            },
        }
    }

    pub fn save_auth(&self) -> Result<()> {
        let str = serde_json::to_string(&self.twitter_auth)?;
        fs::write("./var/.oauth", str)?;
        Ok(())
    }

    pub fn load_auth(&mut self) -> Result<()> {
        let str = fs::read_to_string("./var/.oauth")?;
        self.twitter_auth = serde_json::from_str(&str)?;
        Ok(())
    }

    pub async fn authorize(&mut self, use_refresh_token: bool) -> Result<()> {
        let oauth_client = BasicClient::new(
            ClientId::new(self.twitter_client_id.clone()),
            Some(ClientSecret::new(self.twitter_client_secret.clone())),
            AuthUrl::new("https://twitter.com/i/oauth2/authorize".to_string())?,
            Some(TokenUrl::new(
                "https://api.twitter.com/2/oauth2/token".to_string(),
            )?),
        )
        .set_redirect_uri(RedirectUrl::new("https://localhost:8080".to_string())?);
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let (auth_url, _csrf_token) = oauth_client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("tweet.read".to_string()))
            .add_scope(Scope::new("users.read".to_string()))
            .add_scope(Scope::new("offline.access".to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        match &self.twitter_auth.refresh_token {
            Some(refresh_token) if use_refresh_token => {
                let token = oauth_client
                    .exchange_refresh_token(refresh_token)
                    .request_async(async_http_client)
                    .await?;
                self.twitter_auth.access_token = Some(token.access_token().clone());
                self.twitter_auth.refresh_token = token.refresh_token().cloned();
                self.save_auth()?;
            }
            _ => {
                // User browses here to complete OAuth flow
                process::Command::new("open")
                    .arg(auth_url.to_string())
                    .output()
                    .expect(&format!("Failed to open url in browser: {auth_url}"));

                let mut callback_url = String::new();
                println!("Enter callback url:");
                std::io::stdin().read_line(&mut callback_url)?;
                let callback_url = Url::parse(&callback_url)?;

                // let (set_authorization_code, mut authorization_code) =
                //     tokio::sync::mpsc::channel::<String>(1);
                // let callback_addr = SocketAddr::from(([127, 0, 0, 1], 8080));
                // let callback_listener = TcpListener::bind(callback_addr).await?;

                fn parse_authorization_code(url: &Url) -> Result<String> {
                    let mut expected_csrf_state = None;
                    let mut authorization_code = None;
                    for (key, value) in url.query_pairs() {
                        if key == "state" {
                            expected_csrf_state = Some(String::from(value));
                        } else if key == "code" {
                            authorization_code = Some(String::from(value));
                        }
                    }
                    let _expected_csrf_state = expected_csrf_state
                        .ok_or(anyhow!("Missing `state` param from callback"))?;
                    let authorization_code =
                        authorization_code.ok_or(anyhow!("Missing `code` param from callback"))?;

                    // Once the user has been redirected to the redirect URL, you'll have access to the
                    // authorization code. For security reasons, your code should verify that the `state`
                    // parameter returned by the server matches `csrf_state`.
                    Ok(authorization_code)
                }

                // tokio::task::spawn(async move {
                //     loop {
                //         let (stream, _) = callback_listener.accept().await.unwrap();
                //         if let Err(err) = Http::new()
                //             .serve_connection(
                //                 stream,
                //                 hyper::service::service_fn(|req| {
                //                     let set_authorization_code = set_authorization_code.clone();
                //                     async move {
                //                         let authorization_code = parse_authorization_code(req.uri())?;
                //                         set_authorization_code.send(authorization_code).await?;
                //                         Ok::<_, anyhow::Error>(hyper::Response::new(hyper::Body::from(
                //                             "You can close this window now",
                //                         )))
                //                     }
                //                 }),
                //             )
                //             .await
                //         {
                //             eprintln!("Error serving callback: {}", err);
                //         }
                //     }
                // });

                let authorization_code = parse_authorization_code(&callback_url)?;
                let token_result = oauth_client
                    .exchange_code(AuthorizationCode::new(authorization_code))
                    .set_pkce_verifier(pkce_verifier)
                    .request_async(async_http_client)
                    .await?;

                self.twitter_auth.access_token = Some(token_result.access_token().clone());
                self.twitter_auth.refresh_token = token_result.refresh_token().cloned();
            }
        }
        Ok(())
    }

    async fn authenticated_get(&self, uri: &Url) -> Result<Bytes> {
        let access_token = self
            .twitter_auth
            .access_token
            .as_ref()
            .ok_or(anyhow!("Unauthorized"))?;
        let req = Request::builder()
            .method(Method::GET)
            .uri(uri.to_string())
            .header("Authorization", format!("Bearer {}", access_token.secret()))
            .body(Body::empty())?;
        let resp = self.https_client.request(req).await?;
        let resp = hyper::body::to_bytes(resp.into_body()).await?;
        Ok(resp)
    }

    pub async fn me(&self) -> Result<api::User> {
        let uri = Url::parse("https://api.twitter.com/2/users/me")?;
        let bytes = self.authenticated_get(&uri).await?;
        let resp: api::Response<api::User, ()> = serde_json::from_slice(&bytes)?;
        Ok(resp.data)
    }

    pub async fn user_by_username(&self, username: &str) -> Result<api::User> {
        let mut uri = Url::parse(&format!(
            "https://api.twitter.com/2/users/by/username/{username}"
        ))?;
        uri.query_pairs_mut().append_pair("user.fields", "username");
        let bytes = self.authenticated_get(&uri).await?;
        let resp: api::Response<api::User, ()> = serde_json::from_slice(&bytes)?;
        Ok(resp.data)
    }

    async fn get_tweets_with_users(
        &self,
        uri: &mut Url,
        pagination_token: Option<String>,
    ) -> PagedResult<Vec<api::Tweet>> {
        uri.query_pairs_mut()
            .append_pair(
                "tweet.fields",
                "created_at,attachments,referenced_tweets,public_metrics,conversation_id",
            )
            .append_pair("user.fields", "username")
            .append_pair("expansions", "author_id")
            .append_pair("max_results", "100");
        if let Some(pagination_token) = pagination_token {
            uri.query_pairs_mut()
                .append_pair("pagination_token", &pagination_token);
        }
        let bytes = self.authenticated_get(&uri).await?;

        #[derive(Debug, Serialize, Deserialize)]
        struct Includes {
            users: Vec<api::User>,
        }

        let resp: api::Response<Vec<api::Tweet>, Includes> = serde_json::from_slice(&bytes)?;
        let next_pagination_token = resp.meta.and_then(|meta| meta.next_token);
        let includes = resp.includes.ok_or(anyhow!("Expected `includes`"))?;
        let users: HashMap<String, &api::User> = includes
            .users
            .iter()
            .map(|user| (user.id.clone(), user))
            .collect();
        let tweets: Vec<api::Tweet> = resp
            .data
            .iter()
            .map(|tweet| api::Tweet {
                author_username: users
                    .get(&tweet.author_id)
                    .map(|user| user.username.clone()),
                author_name: users.get(&tweet.author_id).map(|user| user.name.clone()),
                ..tweet.clone()
            })
            .collect();
        Ok((tweets, next_pagination_token))
    }

    pub async fn user_tweets(
        &self,
        user_id: &str,
        pagination_token: Option<String>,
    ) -> PagedResult<Vec<api::Tweet>> {
        let mut uri = Url::parse(&format!("https://api.twitter.com/2/users/{user_id}/tweets"))?;
        self.get_tweets_with_users(&mut uri, pagination_token).await
    }

    pub async fn timeline_reverse_chronological(
        &self,
        user_id: &str,
        pagination_token: Option<String>,
    ) -> PagedResult<Vec<api::Tweet>> {
        let mut uri = Url::parse(&format!(
            "https://api.twitter.com/2/users/{user_id}/timelines/reverse_chronological"
        ))?;
        self.get_tweets_with_users(&mut uri, pagination_token).await
    }

    pub async fn search_tweets(&self, query: &str) -> PagedResult<Vec<api::Tweet>> {
        let mut uri = Url::parse("https://api.twitter.com/2/tweets/search/recent")?;
        uri.query_pairs_mut().append_pair("query", query);
        self.get_tweets_with_users(&mut uri, None).await
    }
}
