pub mod api;

use anyhow::{anyhow, Result};
use hyper::client::HttpConnector;
use hyper::{Body, Client, Method, Request};
use hyper_tls::HttpsConnector;
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AccessToken, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use url::Url;

#[derive(Debug, Clone)]
pub struct TwitterClient {
    https_client: Client<HttpsConnector<HttpConnector>>,
    twitter_client_id: String,
    twitter_client_secret: String,
    access_token: Option<AccessToken>,
}

impl TwitterClient {
    pub fn new(twitter_client_id: &str, twitter_client_secret: &str) -> Self {
        let https = HttpsConnector::new();
        let https_client = Client::builder().build::<_, hyper::Body>(https);
        Self {
            https_client,
            twitter_client_id: twitter_client_id.to_string(),
            twitter_client_secret: twitter_client_secret.to_string(),
            access_token: None,
        }
    }

    pub fn save_access_token(&self) -> Result<()> {
        // CR-soon: do we have to use serde_json, what about plain bytes
        let access_token = self
            .access_token
            .as_ref()
            .ok_or(anyhow!("No token to save"))?;
        let access_token = serde_json::to_string(&access_token)?;
        fs::write("./var/.access_token", access_token)?;
        Ok(())
    }

    pub fn load_access_token(&mut self) -> Result<()> {
        let access_token = fs::read_to_string("./var/.access_token")?;
        let access_token = serde_json::from_str(&access_token)?;
        self.access_token = Some(access_token);
        Ok(())
    }

    pub async fn authorize(&mut self) -> Result<()> {
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

        // User browses here to complete OAuth flow
        println!("Browse to: {auth_url}");

        let mut callback_url = String::new();
        println!("Enter callback url:");
        std::io::stdin().read_line(&mut callback_url)?;
        let callback_url = Url::parse(&callback_url)?;

        let mut expected_csrf_state = None;
        let mut authorization_code = None;

        for (key, value) in callback_url.query_pairs() {
            if key == "state" {
                expected_csrf_state = Some(String::from(value));
            } else if key == "code" {
                authorization_code = Some(String::from(value));
            }
        }

        let _expected_csrf_state =
            expected_csrf_state.ok_or(anyhow!("Missing `state` param from callback"))?;
        let authorization_code =
            authorization_code.ok_or(anyhow!("Missing `code` param from callback"))?;

        // Once the user has been redirected to the redirect URL, you'll have access to the
        // authorization code. For security reasons, your code should verify that the `state`
        // parameter returned by the server matches `csrf_state`.
        let token_result = oauth_client
            .exchange_code(AuthorizationCode::new(authorization_code))
            .set_pkce_verifier(pkce_verifier)
            .request_async(async_http_client)
            .await?;

        self.access_token = Some(token_result.access_token().clone());
        Ok(())
    }

    pub async fn me(&self) -> Result<api::User> {
        let access_token = self.access_token.as_ref().ok_or(anyhow!("Unauthorized"))?;
        let req = Request::builder()
            .method(Method::GET)
            .uri("https://api.twitter.com/2/users/me")
            .header("Authorization", format!("Bearer {}", access_token.secret()))
            .body(Body::empty())?;

        let resp = self.https_client.request(req).await?;
        let resp = hyper::body::to_bytes(resp.into_body()).await?;
        let resp: api::Response<api::User, ()> = serde_json::from_slice(&resp)?;
        Ok(resp.data)
    }

    pub async fn timeline_reverse_chronological(
        &self,
        user_id: &str,
        pagination_token: Option<&String>,
    ) -> Result<(Vec<api::Tweet>, Option<String>)> {
        let access_token = self.access_token.as_ref().ok_or(anyhow!("Unauthorized"))?;

        let mut uri = Url::parse(&format!(
            "https://api.twitter.com/2/users/{user_id}/timelines/reverse_chronological"
        ))?;

        uri.query_pairs_mut()
            .append_pair(
                "tweet.fields",
                "created_at,attachments,referenced_tweets,public_metrics,conversation_id",
            )
            .append_pair("user.fields", "username")
            .append_pair("expansions", "author_id");

        if let Some(pagination_token) = pagination_token {
            uri.query_pairs_mut()
                .append_pair("pagination_token", pagination_token);
        }

        let req = Request::builder()
            .method(Method::GET)
            .uri(uri.to_string())
            .header("Authorization", format!("Bearer {}", access_token.secret()))
            .body(Body::empty())?;

        #[derive(Debug, Serialize, Deserialize)]
        struct Includes {
            users: Vec<api::User>,
        }

        let resp = self.https_client.request(req).await?;
        let resp = hyper::body::to_bytes(resp.into_body()).await?;
        let resp: api::Response<Vec<api::Tweet>, Includes> = serde_json::from_slice(&resp)?;

        let includes = resp.includes.ok_or(anyhow!("Expected `includes`"))?;
        let users: HashMap<String, &api::User> = includes
            .users
            .iter()
            .map(|user| (user.id.clone(), user))
            .collect();

        // CR: does Cow help here vs Clone?
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

        let next_pagination_token = resp.meta.and_then(|meta| meta.next_token);

        Ok((tweets, next_pagination_token))
    }
}
