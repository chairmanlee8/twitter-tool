use dotenvy::dotenv;
use std::env;
use twitter_tool_rs::client::TwitterClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenv().ok();

    let twitter_client_id = env::var("TWITTER_CLIENT_ID")?;
    let twitter_client_secret = env::var("TWITTER_CLIENT_SECRET")?;
    let mut twitter_client = TwitterClient::new(&twitter_client_id, &twitter_client_secret);

    twitter_client.authorize().await?;

    let me = twitter_client.me().await?;
    println!("{me:?}");

    let my_timeline = twitter_client
        .timeline_reverse_chronological(&me.id)
        .await?;
    println!("{my_timeline:?}");

    Ok(())
}
