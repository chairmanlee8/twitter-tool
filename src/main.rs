use clap::Parser;
use dotenvy::dotenv;
use std::env;
use twitter_tool_rs::twitter_client::TwitterClient;
use twitter_tool_rs::ui;
use anyhow::Result;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    login: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    dotenv().ok();

    let twitter_client_id = env::var("TWITTER_CLIENT_ID")?;
    let twitter_client_secret = env::var("TWITTER_CLIENT_SECRET")?;
    let mut twitter_client = TwitterClient::new(&twitter_client_id, &twitter_client_secret);

    if args.login {
        twitter_client.authorize().await?;
        twitter_client.save_access_token()?;
    } else {
        twitter_client.load_access_token()?;
    }

    let me = twitter_client.me().await?;
    println!("{me:?}");

    let mut ui = ui::UI::new(twitter_client, me);
    ui.event_loop().await
}
