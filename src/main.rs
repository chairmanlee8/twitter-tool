use anyhow::Result;
use clap::Parser;
use dotenvy::dotenv;
use std::convert::Infallible;
use std::{env, fs, io};
use twitter_tool_rs::twitter_client::TwitterClient;
use twitter_tool_rs::ui;
use twitter_tool_rs::user_config::UserConfig;

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
    twitter_client.load_auth().or_else(|_| {
        eprintln!("No auth file found, must login");
        Ok::<_, Infallible>(())
    })?;
    twitter_client.authorize(!args.login).await?;
    twitter_client.save_auth()?;

    let me = twitter_client.me().await?;
    println!("{me:?}");

    let user_config = match fs::read_to_string("./var/.user_config") {
        Ok(file_contents) => serde_json::from_str::<UserConfig>(&file_contents)?,
        Err(err) if err.kind() == io::ErrorKind::NotFound => UserConfig::default(),
        Err(err) => panic!("Error reading user config: {:?}", err),
    };

    let mut ui = ui::UI::new(twitter_client, &me, &user_config);
    ui.initialize();
    ui.event_loop().await
}
