mod bottom_bar;
mod feed_pane;
mod tweet_pane;

use crate::twitter_client::{api, TwitterClient};
use crate::ui::bottom_bar::BottomBar;
use crate::ui::feed_pane::FeedPane;
use crate::ui::tweet_pane::TweetPane;
use anyhow::{anyhow, Context, Error, Result};
use crossterm::cursor;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent};
use crossterm::terminal;
use crossterm::{
    execute, queue,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::{FutureExt, StreamExt};
use std::collections::HashMap;
use std::fs;
use std::io::{stdout, Stdout, Write};
use std::process;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Mode {
    Log,
    Interactive,
}

#[derive(Debug)]
pub enum InternalEvent {
    FeedRepaint,
    SelectTweet(String),
    LogTweet(String),
    LogError(Error),
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct BoundingBox {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
}

impl BoundingBox {
    pub fn new(left: u16, top: u16, width: u16, height: u16) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }
}

pub trait Render {
    // NB: [render] takes [&mut self] since there isn't a separate notification to component that
    // their bbox changed
    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()>;
}

pub trait Input {
    fn handle_key_event(&mut self, event: KeyEvent);
    fn get_cursor(&self, bounding_box: BoundingBox) -> (u16, u16);
}

// CR-someday: pub trait Animate (or maybe combine with Render)

struct Component<T: Render + Input> {
    pub should_render: bool,
    pub bounding_box: BoundingBox,
    pub component: T,
}

impl<T: Render + Input> Component<T> {
    pub fn new(component: T) -> Self {
        Self {
            should_render: true,
            bounding_box: BoundingBox::default(),
            component,
        }
    }

    pub fn render_if_necessary(&mut self, stdout: &mut Stdout) -> Result<()> {
        if self.should_render {
            self.component.render(stdout, self.bounding_box)?;
            self.should_render = false;
        }
        Ok(())
    }

    pub fn get_cursor(&self) -> (u16, u16) {
        self.component.get_cursor(self.bounding_box)
    }
}

// TODO: enum methods?
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Focus {
    FeedPane,
    TweetPane,
}

impl Focus {
    pub fn next(&self) -> Self {
        match self {
            Self::FeedPane => Self::TweetPane,
            Self::TweetPane => Self::FeedPane,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::FeedPane => Self::TweetPane,
            Self::TweetPane => Self::FeedPane,
        }
    }
}

// TODO deep dive into str vs String
pub struct UI {
    stdout: Stdout,
    mode: Mode,
    events: (
        UnboundedSender<InternalEvent>,
        UnboundedReceiver<InternalEvent>,
    ),
    feed_pane: Component<FeedPane>,
    tweet_pane: Component<TweetPane>,
    bottom_bar: Component<BottomBar>,
    focus: Focus,
    twitter_client: Arc<TwitterClient>,
    twitter_user: Arc<api::User>,
    tweets: Arc<Mutex<HashMap<String, api::Tweet>>>,
    tweets_reverse_chronological: Arc<Mutex<Vec<String>>>,
    tweets_page_token: Arc<AsyncMutex<Option<String>>>,
}

impl UI {
    pub fn new(twitter_client: TwitterClient, twitter_user: api::User) -> Self {
        let (cols, rows) = terminal::size().unwrap();
        let (tx, rx) = mpsc::unbounded_channel();

        let tweets = Arc::new(Mutex::new(HashMap::new()));
        let tweets_reverse_chronological = Arc::new(Mutex::new(Vec::new()));

        let feed_pane = FeedPane::new(&tx, &tweets, &tweets_reverse_chronological);
        let tweet_pane = TweetPane::new(&tweets);
        let bottom_bar = BottomBar::new(&tweets_reverse_chronological);

        let mut this = Self {
            stdout: stdout(),
            mode: Mode::Log,
            events: (tx, rx),
            feed_pane: Component::new(feed_pane),
            tweet_pane: Component::new(tweet_pane),
            bottom_bar: Component::new(bottom_bar),
            focus: Focus::FeedPane,
            twitter_client: Arc::new(twitter_client),
            twitter_user: Arc::new(twitter_user),
            tweets,
            tweets_reverse_chronological,
            tweets_page_token: Arc::new(AsyncMutex::new(None)),
        };

        this.resize(cols, rows);
        this
    }

    fn set_mode(&mut self, mode: Mode) -> Result<()> {
        let prev_mode = self.mode;
        self.mode = mode;

        if prev_mode == Mode::Log && mode == Mode::Interactive {
            execute!(self.stdout, EnterAlternateScreen)?;
            terminal::enable_raw_mode()?;
        } else if prev_mode == Mode::Interactive && mode == Mode::Log {
            execute!(self.stdout, LeaveAlternateScreen)?;
            terminal::enable_raw_mode()?;
            // CR: disabling raw mode entirely also gets rid of the keypress events...
            // disable_raw_mode()?;
        }

        Ok(())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let half_width = cols / 2;
        self.feed_pane.bounding_box = BoundingBox::new(0, 0, half_width - 1, rows - 2);
        self.tweet_pane.bounding_box = BoundingBox::new(half_width + 1, 0, half_width, rows - 2);
        self.bottom_bar.bounding_box = BoundingBox::new(0, rows - 1, cols, 1);
    }

    pub async fn render(&mut self) -> Result<()> {
        self.set_mode(Mode::Interactive)?;

        self.feed_pane.render_if_necessary(&mut self.stdout)?;
        self.tweet_pane.render_if_necessary(&mut self.stdout)?;
        self.bottom_bar.render_if_necessary(&mut self.stdout)?;

        let focus = match self.focus {
            Focus::FeedPane => self.feed_pane.get_cursor(),
            Focus::TweetPane => self.tweet_pane.get_cursor(),
        };
        queue!(&self.stdout, cursor::MoveTo(focus.0, focus.1))?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn log_message(&mut self, message: &str) -> Result<()> {
        self.set_mode(Mode::Log)?;
        println!("{message}\r");
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
                    let mut tweets = tweets.lock().unwrap();
                    for tweet in new_tweets {
                        new_tweets_reverse_chronological.push(tweet.id.clone());
                        tweets.insert(tweet.id.clone(), tweet);
                    }
                }
                {
                    let mut tweets_reverse_chronological =
                        tweets_reverse_chronological.lock().unwrap();
                    tweets_reverse_chronological.append(&mut new_tweets_reverse_chronological);
                }
                Ok(())
            }
            .await
            {
                Ok(()) => event_sender.send(InternalEvent::FeedRepaint),
                Err(error) => event_sender.send(InternalEvent::LogError(error)),
            }
        });
    }

    async fn handle_internal_event(&mut self, event: InternalEvent) -> Result<()> {
        match event {
            InternalEvent::FeedRepaint => {
                self.feed_pane.should_render = true;
                self.bottom_bar.should_render = true;
                self.render().await?;
            }
            InternalEvent::SelectTweet(tweet_id) => {
                self.tweet_pane
                    .component
                    .set_selected_tweet_id(Some(tweet_id));
                self.tweet_pane.should_render = true;
                self.render().await?;
            }
            InternalEvent::LogTweet(tweet_id) => {
                {
                    let tweets = self.tweets.lock().unwrap();
                    let tweet = &tweets[&tweet_id];
                    fs::write("/tmp/tweet", format!("{:#?}", tweet))?;
                }

                let mut subshell = process::Command::new("less").args(["/tmp/tweet"]).spawn()?;
                subshell.wait()?;
            }
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
                    self.feed_pane.should_render = true;
                    self.tweet_pane.should_render = true;
                    self.bottom_bar.should_render = true;
                    self.render().await?
                }
                KeyCode::Tab => {
                    self.focus = self.focus.next();
                    self.render().await?
                }
                KeyCode::Char('h') => self.log_message("hello")?,
                KeyCode::Char('n') => {
                    self.do_load_page_of_tweets(false);
                }
                KeyCode::Char('q') => {
                    reset();
                    process::exit(0);
                }
                _ => match self.focus {
                    Focus::FeedPane => self.feed_pane.component.handle_key_event(key_event),
                    Focus::TweetPane => self.tweet_pane.component.handle_key_event(key_event),
                },
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
