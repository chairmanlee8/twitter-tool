use clap::Parser;
use crossterm::event::{read, Event, KeyCode};
use dotenvy::dotenv;
use std::env;
use twitter_tool_rs::twitter_client::TwitterClient;
use twitter_tool_rs::ui;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    login: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    let (mut tweets, mut next_pagination_token) = twitter_client
        .timeline_reverse_chronological(&me.id, None)
        .await?;

    let mut ui = ui::UI::new();
    ui.set_tweets(&tweets);
    ui.show_tweets()?;

    loop {
        // CR: TODO in general, only the UI knows the meaning of keystrokes
        match read()? {
            Event::Key(key_event) => match key_event.code {
                KeyCode::Esc => ui.show_tweets()?,
                KeyCode::Up => ui.move_selected_index(-1)?,
                KeyCode::Down => ui.move_selected_index(1)?,
                KeyCode::Char('i') => ui.log_selected_tweet()?,
                KeyCode::Char('n') => {
                    if let Some(pagination_token) = next_pagination_token {
                        let (mut more_tweets, pagination_token) = twitter_client
                            .timeline_reverse_chronological(&me.id, Some(&pagination_token))
                            .await?;
                        next_pagination_token = pagination_token;
                        tweets.append(&mut more_tweets);
                        ui.set_tweets(&tweets);
                        ui.show_tweets()?;
                    } else {
                        ui.log_message("No more pages")?
                    }
                },
                KeyCode::Char('q') => {
                    ui::reset();
                    std::process::exit(0);
                }
                _ => (),
            },
            Event::Resize(cols, rows) => ui.resize(cols, rows),
            _ => (),
        }
    }
}
