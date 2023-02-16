mod bottom_bar;
mod feed_pane;
mod tweet_pane;

use crate::twitter_client::{api, TwitterClient};
use crate::ui::bottom_bar::render_bottom_bar;
use crate::ui::feed_pane::render_feed_pane;
use crate::ui::tweet_pane::render_tweet_pane;
use anyhow::{anyhow, Context, Error, Result};
use bitflags::bitflags;
use crossterm::cursor;
use crossterm::event::{Event, EventStream, KeyCode};
use crossterm::terminal;
use crossterm::{
    execute, queue,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::{FutureExt, StreamExt};
use std::cmp::{max, min};
use std::collections::HashMap;
use std::fs;
use std::io::{stdout, Stdout, Write};
use std::process;
use std::sync::Arc;
use tokio::sync::{
    mpsc::{self, UnboundedReceiver, UnboundedSender},
    Mutex,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Mode {
    Log,
    Interactive,
}

pub struct Layout {
    pub stdout: Stdout,
    pub screen_cols: u16,
    pub screen_rows: u16,
    pub feed_pane_width: u16,
    pub tweet_pane_width: u16,
}

#[derive(Debug)]
enum InternalEvent {
    TweetsFeedUpdated,
    LogError(Error),
}

bitflags! {
    struct Dirty: u8 {
        const FEED_PANE = 1 << 0;
        const TWEET_PANE = 1 << 1;
        const BOTTOM_BAR = 1 << 2;
    }
}

// TODO deep dive into str vs String
pub struct UI {
    mode: Mode,
    layout: Layout,
    events: (
        UnboundedSender<InternalEvent>,
        UnboundedReceiver<InternalEvent>,
    ),
    dirty: Dirty,
    twitter_client: Arc<TwitterClient>,
    twitter_user: Arc<api::User>,
    tweets: Arc<Mutex<HashMap<String, api::Tweet>>>,
    tweets_reverse_chronological: Arc<Mutex<Vec<String>>>,
    tweets_page_token: Arc<Mutex<Option<String>>>,
    tweets_view_offset: usize,
    tweets_selected_index: usize,
}

impl UI {
    pub fn new(twitter_client: TwitterClient, twitter_user: api::User) -> Self {
        let (cols, rows) = terminal::size().unwrap();
        let (tx, rx) = mpsc::unbounded_channel();

        Self {
            mode: Mode::Log,
            layout: Layout {
                stdout: stdout(),
                screen_cols: cols,
                screen_rows: rows,
                feed_pane_width: cols / 2,
                tweet_pane_width: cols / 2,
            },
            events: (tx, rx),
            dirty: Dirty::all(),
            twitter_client: Arc::new(twitter_client),
            twitter_user: Arc::new(twitter_user),
            tweets: Arc::new(Mutex::new(HashMap::new())),
            tweets_reverse_chronological: Arc::new(Mutex::new(Vec::new())),
            tweets_page_token: Arc::new(Mutex::new(None)),
            tweets_view_offset: 0,
            tweets_selected_index: 0,
        }
    }

    fn set_mode(&mut self, mode: Mode) -> Result<()> {
        let prev_mode = self.mode;
        self.mode = mode;

        if prev_mode == Mode::Log && mode == Mode::Interactive {
            execute!(stdout(), EnterAlternateScreen)?;
            terminal::enable_raw_mode()?;
        } else if prev_mode == Mode::Interactive && mode == Mode::Log {
            execute!(stdout(), LeaveAlternateScreen)?;
            terminal::enable_raw_mode()?;
            // CR: disabling raw mode entirely also gets rid of the keypress events...
            // disable_raw_mode()?;
        }

        Ok(())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.layout.screen_cols = cols;
        self.layout.screen_rows = rows;
    }

    pub async fn move_selected_index(&mut self, delta: isize) -> Result<()> {
        {
            let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().await;

            let new_index = max(0, self.tweets_selected_index as isize + delta) as usize;
            let new_index = min(new_index, tweets_reverse_chronological.len() - 1);
            let view_top = self.tweets_view_offset;
            let view_height = (self.layout.screen_rows - 3) as usize;
            let view_bottom = self.tweets_view_offset + view_height;

            self.tweets_selected_index = new_index;

            if new_index < view_top {
                self.tweets_view_offset = new_index;
                self.dirty = Dirty::all();
            } else if new_index > view_bottom {
                self.tweets_view_offset = max(0, new_index - view_height);
                self.dirty = Dirty::all();
            } else {
                self.dirty.insert(Dirty::BOTTOM_BAR | Dirty::TWEET_PANE);
            }
        }

        self.render().await
    }

    pub async fn render(&mut self) -> Result<()> {
        self.set_mode(Mode::Interactive)?;

        {
            let tweets = self.tweets.lock().await;
            let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().await;

            // TODO: we might have enough to make Component trait now, that way we can keep
            // dirty/focus return per widget
            if self.dirty.contains(Dirty::FEED_PANE) {
                render_feed_pane(
                    &self.layout,
                    &tweets,
                    &tweets_reverse_chronological,
                    self.tweets_view_offset,
                )?;
            }

            if self.dirty.contains(Dirty::TWEET_PANE) {
                render_tweet_pane(
                    &self.layout,
                    &tweets[&tweets_reverse_chronological[self.tweets_selected_index]],
                )?;
            }

            if self.dirty.contains(Dirty::BOTTOM_BAR) {
                render_bottom_bar(
                    &self.layout,
                    &tweets_reverse_chronological,
                    self.tweets_selected_index,
                )?;
            }

            self.dirty = Dirty::empty();
        }

        let mut stdout = &self.layout.stdout;
        queue!(
            &self.layout.stdout,
            cursor::MoveTo(
                16,
                (self.tweets_selected_index - self.tweets_view_offset) as u16
            )
        )?;
        stdout.flush()?;
        Ok(())
    }

    pub async fn log_selected_tweet(&mut self) -> Result<()> {
        {
            let tweets = self.tweets.lock().await;
            let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().await;
            let tweet_id = &tweets_reverse_chronological[self.tweets_selected_index];
            let tweet = &tweets[tweet_id];
            fs::write("/tmp/tweet", format!("{:#?}", tweet))?;
        }

        let mut subshell = process::Command::new("less").args(["/tmp/tweet"]).spawn()?;
        subshell.wait()?;
        Ok(())
    }

    pub fn log_message(&mut self, message: &str) -> Result<()> {
        self.set_mode(Mode::Log)?;
        println!("{message}");
        Ok(())
    }

    // CR: need to sift results
    // CR: need a fixed page size, then call the twitterclient as many times as needed to achieve
    // the desired page effect
    pub fn do_load_page_of_tweets(&mut self, restart: bool) {
        let event_sender = self.events.0.clone();
        let twitter_client = self.twitter_client.clone();
        let twitter_user = self.twitter_user.clone();
        let tweets_page_token = self.tweets_page_token.clone();
        let tweets = self.tweets.clone();
        let tweets_reverse_chronological = self.tweets_reverse_chronological.clone();

        tokio::spawn(async move {
            match async {
                let mut tweets_page_token = tweets_page_token
                    .try_lock()
                    .with_context(|| "Cannot get lock")?;
                // NB: require page token if continuing to next page
                let maybe_page_token = match restart {
                    true => Ok::<Option<&String>, Error>(None),
                    false => {
                        let page_token =
                            tweets_page_token.as_ref().ok_or(anyhow!("No more pages"))?;
                        Ok(Some(page_token))
                    }
                }?;
                let (new_tweets, page_token) = twitter_client
                    .timeline_reverse_chronological(&twitter_user.id, maybe_page_token)
                    .await?;
                let mut new_tweets_reverse_chronological: Vec<String> = Vec::new();

                *tweets_page_token = page_token;

                {
                    let mut tweets = tweets.lock().await;
                    for tweet in new_tweets {
                        new_tweets_reverse_chronological.push(tweet.id.clone());
                        tweets.insert(tweet.id.clone(), tweet);
                    }
                }
                {
                    let mut tweets_reverse_chronological =
                        tweets_reverse_chronological.lock().await;
                    tweets_reverse_chronological.append(&mut new_tweets_reverse_chronological);
                }
                Ok(())
            }
            .await
            {
                Ok(()) => event_sender.send(InternalEvent::TweetsFeedUpdated),
                Err(error) => event_sender.send(InternalEvent::LogError(error)),
            }
        });
    }

    async fn handle_internal_event(&mut self, event: InternalEvent) -> Result<()> {
        match event {
            InternalEvent::TweetsFeedUpdated => {
                self.dirty.insert(Dirty::TWEET_PANE);
                self.render().await?;
            },
            InternalEvent::LogError(err) => {
                self.log_message(err.to_string().as_str())?;
            }
        }
        Ok(())
    }

    async fn handle_terminal_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key_event) => match key_event.code {
                KeyCode::Esc => {
                    self.dirty = Dirty::all();
                    self.render().await?
                },
                KeyCode::Up => self.move_selected_index(-1).await?,
                KeyCode::Down => self.move_selected_index(1).await?,
                KeyCode::Char('h') => self.log_message("hello")?,
                KeyCode::Char('i') => self.log_selected_tweet().await?,
                KeyCode::Char('n') => {
                    self.do_load_page_of_tweets(false);
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
        Ok(())
    }

    pub async fn event_loop(&mut self) -> Result<()> {
        let mut terminal_event_stream = EventStream::new();

        loop {
            let terminal_event = terminal_event_stream.next().fuse();
            let internal_event = self.events.1.recv();

            tokio::select! {
                event = terminal_event => {
                    if let Some(Ok(event)) = event {
                        self.handle_terminal_event(event).await?;
                    }
                },
                event = internal_event => {
                    if let Some(event) = event {
                        self.handle_internal_event(event).await?;
                    }
                }
            }
        }
    }
}

pub fn reset() {
    execute!(stdout(), LeaveAlternateScreen).unwrap();
    terminal::disable_raw_mode().unwrap()
}
