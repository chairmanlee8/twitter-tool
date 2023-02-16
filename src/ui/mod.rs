mod bottom_bar;
mod tweet_pane;
mod tweets_pane;

use crate::ui::bottom_bar::render_bottom_bar;
use crate::ui::tweets_pane::render_tweets_pane;
use crossterm::event::{read, Event, KeyCode};
use crossterm::cursor;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size, Clear, ClearType};
use crossterm::{
    execute, queue,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use std::cmp::{max, min};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::process;
use anyhow::{anyhow, Result};
use std::io::{ErrorKind, stdout, Write};
use std::sync::{Arc};
use tokio::sync::Mutex;
use crate::twitter_client::{api, TwitterClient};
use crate::ui::tweet_pane::render_tweet_pane;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Mode {
    Log,
    Interactive,
}

pub struct Context {
    pub screen_cols: u16,
    pub screen_rows: u16,
    // TODO pub tweets_feed_pane_width, pub tweet_detail_pane_width, rename bottom_bar to status_bar
}

// TODO deep dive into str vs String
pub struct UI {
    mode: Mode,
    context: Context,
    twitter_client: Arc<TwitterClient>,
    twitter_user: Arc<api::User>,
    tweets: Arc<Mutex<HashMap<String, api::Tweet>>>,
    tweets_reverse_chronological: Arc<Mutex<Vec<String>>>,
    tweets_page_token: Arc<Mutex<Option<String>>>,
    tweets_view_offset: usize,
    tweets_selected_index: usize,
    tweet_pane_width: u16
}

// TODO can we split impl?
impl UI {
    pub fn new(twitter_client: TwitterClient, twitter_user: api::User) -> Self {
        let (cols, rows) = size().unwrap();

        Self {
            mode: Mode::Log,
            context: Context {
                screen_cols: cols,
                screen_rows: rows,
            },
            twitter_client: Arc::new(twitter_client),
            twitter_user: Arc::new(twitter_user),
            tweets: Arc::new(Mutex::new(HashMap::new())),
            tweets_reverse_chronological: Arc::new(Mutex::new(Vec::new())),
            tweets_page_token: Arc::new(Mutex::new(None)),
            tweets_view_offset: 0,
            tweets_selected_index: 0,
            tweet_pane_width: 80
        }
    }

    fn set_mode(&mut self, mode: Mode) -> Result<()> {
        let prev_mode = self.mode;
        self.mode = mode;

        if prev_mode == Mode::Log && mode == Mode::Interactive {
            execute!(stdout(), EnterAlternateScreen)?;
            enable_raw_mode()?;
        } else if prev_mode == Mode::Interactive && mode == Mode::Log {
            execute!(stdout(), LeaveAlternateScreen)?;
            enable_raw_mode()?;
            // CR: disabling raw mode entirely also gets rid of the keypress events...
            // disable_raw_mode()?;
        }

        Ok(())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.context = Context {
            screen_cols: cols,
            screen_rows: rows,
        };
    }

    pub async fn move_selected_index(&mut self, delta: isize) -> Result<()> {
        let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().await;

        let new_index = max(0, self.tweets_selected_index as isize + delta) as usize;
        let new_index = min(new_index, tweets_reverse_chronological.len() - 1);
        let view_top = self.tweets_view_offset;
        let view_height = (self.context.screen_rows - 3) as usize;
        let view_bottom = self.tweets_view_offset + view_height;

        self.tweets_selected_index = new_index;

        drop(tweets_reverse_chronological);

        if new_index < view_top {
            self.tweets_view_offset = new_index;
            self.show_tweets().await
        } else if new_index > view_bottom {
            self.tweets_view_offset = max(0, new_index - view_height);
            self.show_tweets().await
        } else {
            // CR: this is confusing re the above two conditions, consider a refactor
            {
                let tweets = self.tweets.lock().await;
                let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().await;

                render_tweet_pane(
                    &self.context,
                    self.tweet_pane_width,
                    &tweets[&tweets_reverse_chronological[self.tweets_selected_index]]
                )?;

                render_bottom_bar(
                    &self.context,
                    &tweets_reverse_chronological,
                    self.tweets_selected_index
                )?;
            }

            execute!(
                stdout(),
                cursor::MoveTo(
                    16,
                    (self.tweets_selected_index - self.tweets_view_offset) as u16
                )
            )?;

            Ok(())
        }
    }

    // CR: switch to render_tweets (show_tweets then just sets state)
    pub async fn show_tweets(&mut self) -> Result<()> {
        self.set_mode(Mode::Interactive)?;

        queue!(stdout(), Clear(ClearType::All))?;

        let tweets = self.tweets.lock().await;
        let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().await;

        render_tweets_pane(
            &self.context,
            self.context.screen_cols - self.tweet_pane_width - 2,
            &tweets,
            &tweets_reverse_chronological,
            self.tweets_view_offset
        )?;

        render_tweet_pane(
            &self.context,
            self.tweet_pane_width,
            &tweets[&tweets_reverse_chronological[self.tweets_selected_index]]
        )?;

        render_bottom_bar(
            &self.context,
            &tweets_reverse_chronological,
            self.tweets_selected_index
        )?;

        drop(tweets_reverse_chronological);
        drop(tweets);

        queue!(
            stdout(),
            cursor::MoveTo(
                16,
                (self.tweets_selected_index - self.tweets_view_offset) as u16
            )
        )?;

        stdout().flush()?;
        Ok(())
    }

    pub async fn log_selected_tweet(&mut self) -> Result<()> {
        let tweets = self.tweets.lock().await;
        let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().await;

        let tweet_id = &tweets_reverse_chronological[self.tweets_selected_index];
        let tweet = &tweets[tweet_id];
        fs::write("/tmp/tweet", format!("{:#?}", tweet))?;

        drop(tweets_reverse_chronological);
        drop(tweets);

        let mut subshell = process::Command::new("less").args(["/tmp/tweet"]).spawn()?;
        subshell.wait()?;
        Ok(())
    }

    pub fn log_message(&mut self, message: &str) -> Result<()> {
        self.set_mode(Mode::Log)?;
        println!("{message}");
        Ok(())
    }

    // TODO: make these threaded as well
    pub async fn load_first_page_of_tweets(&mut self) -> Result<()> {
        let mut lock = self.tweets_page_token.try_lock();
        // CR: TODO wtf is all this
        if let Ok(ref mut mutex) = lock {
            let (new_tweets, page_token) = self.twitter_client
                .timeline_reverse_chronological(&self.twitter_user.id, None)
                .await?;
            let mut new_tweets_reverse_chronological: Vec<String> = Vec::new();

            **mutex = page_token;

            {
                // CR: TODO safe to unwrap mutex lock?
                let mut tweets = self.tweets.lock().await;
                for tweet in new_tweets {
                    new_tweets_reverse_chronological.push(tweet.id.clone());
                    tweets.insert(tweet.id.clone(), tweet);
                }
            }
            {
                let mut tweets_reverse_chronological = self.tweets_reverse_chronological.lock().await;
                *tweets_reverse_chronological = new_tweets_reverse_chronological;
            }
        }
        Ok(())
    }

    pub fn load_next_page_of_tweets(&mut self) -> Result<()> {
        let twitter_client = self.twitter_client.clone();
        let twitter_user = self.twitter_user.clone();
        let tweets_page_token = self.tweets_page_token.clone();
        let tweets = self.tweets.clone();
        let tweets_reverse_chronological = self.tweets_reverse_chronological.clone();

        tokio::spawn(async move {
            let result = {
                let mut lock = tweets_page_token.try_lock();
                if let Ok(ref mut mutex) = lock {
                    // CR: gross
                    let page_token: Option<String> = mutex.clone();
                    if let Some(page_token) = page_token {
                        // TODO: this is dup code
                        let (new_tweets, page_token) = twitter_client
                            .timeline_reverse_chronological(&twitter_user.id, Some(&page_token))
                            .await?;
                        let mut new_tweets_reverse_chronological: Vec<String> = Vec::new();

                        **mutex = page_token;

                        // CR: TODO safe to unwrap mutex lock?
                        let mut tweets = tweets.lock().await;
                        for tweet in new_tweets {
                            new_tweets_reverse_chronological.push(tweet.id.clone());
                            tweets.insert(tweet.id.clone(), tweet);
                        }
                        drop(tweets);

                        let mut tweets_reverse_chronological = tweets_reverse_chronological.lock().await;
                        tweets_reverse_chronological.append(&mut new_tweets_reverse_chronological);
                        drop(tweets_reverse_chronological);
                    } else {
                        return Err(anyhow!("No more pages"));
                    }
                } else {
                    return Err(anyhow!("Could not get lock"));
                }
                Ok(())
            };
            result
            // match result {
            //     Ok(()) => self.show_tweets()?,
            //     Err(msg) => self.log_message(msg)?
            // };
        });
        Ok(())
    }

    pub async fn event_loop(&mut self) -> Result<()> {
        self.load_first_page_of_tweets().await?;
        self.show_tweets().await?;

        loop {
            match read()? {
                Event::Key(key_event) => match key_event.code {
                    KeyCode::Esc => self.show_tweets().await?,
                    KeyCode::Up => self.move_selected_index(-1).await?,
                    KeyCode::Down => self.move_selected_index(1).await?,
                    KeyCode::Char('i') => self.log_selected_tweet().await?,
                    KeyCode::Char('n') => {
                        self.load_next_page_of_tweets();
                    },
                    KeyCode::Char('q') => {
                        reset();
                        process::exit(0);
                    }
                    _ => (),
                },
                Event::Resize(cols, rows) => self.resize(cols, rows),
                _ => (),
            }
        }
    }
}

pub fn reset() {
    execute!(stdout(), LeaveAlternateScreen).unwrap();
    disable_raw_mode().unwrap()
}
