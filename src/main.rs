use dotenvy::dotenv;
use hyper::body::HttpBody as _;
use hyper::{Body, Client, Method, Request};
use hyper_tls::HttpsConnector;
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse, TokenUrl,
};
use std::env;
use tokio::io::{stdout, AsyncWriteExt as _};
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenv().ok();

    let twitter_client_id = env::var("TWITTER_CLIENT_ID")?;
    let twitter_client_secret = env::var("TWITTER_CLIENT_SECRET")?;

    let oauth_client = BasicClient::new(
        ClientId::new(twitter_client_id),
        Some(ClientSecret::new(twitter_client_secret)),
        AuthUrl::new("https://twitter.com/i/oauth2/authorize".to_string())?,
        Some(TokenUrl::new(
            "https://api.twitter.com/2/oauth2/token".to_string(),
        )?),
    )
    .set_redirect_uri(RedirectUrl::new("https://localhost:8080".to_string())?);
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf_token) = oauth_client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("tweet.read".to_string()))
        .add_scope(Scope::new("users.read".to_string()))
        .add_scope(Scope::new("offline.access".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    // User browses here to complete OAuth flow
    println!("Browse to: {}", auth_url);

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

    let expected_csrf_state = expected_csrf_state.ok_or("Missing `state` param from callback")?;
    let authorization_code = authorization_code.ok_or("Missing `code` param from callback")?;

    // Once the user has been redirected to the redirect URL, you'll have access to the
    // authorization code. For security reasons, your code should verify that the `state`
    // parameter returned by the server matches `csrf_state`.
    let token_result = oauth_client
        .exchange_code(AuthorizationCode::new(authorization_code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let req = Request::builder()
        .method(Method::GET)
        .uri("https://api.twitter.com/2/users/me")
        .header("Authorization", format!("Bearer {}", token_result.access_token().secret()))
        .body(Body::empty())?;
    let mut resp = client.request(req).await?;

    println!("Response: {}", resp.status());

    while let Some(chunk) = resp.body_mut().data().await {
        stdout().write_all(&chunk?).await?;
    }

    Ok(())
}
