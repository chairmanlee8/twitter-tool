mod bottom_bar;
mod feed_pane;
mod search_bar;
mod tweet_pane;
mod tweet_pane_stack;

use crate::store::Store;
use crate::twitter_client::{api, TwitterClient};
use crate::ui::bottom_bar::BottomBar;
use crate::ui::feed_pane::FeedPane;
use crate::ui::tweet_pane::TweetPane;
use crate::ui_framework::bounding_box::BoundingBox;
use crate::ui_framework::{Component, Input, Render};
use crate::user_config::UserConfig;
use anyhow::{anyhow, Context, Error, Result};
use crossterm::cursor;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent};
use crossterm::terminal;
use crossterm::{
    execute, queue,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::stream::FuturesUnordered;
use futures_util::{FutureExt, StreamExt};
use std::fs;
use std::io::{stdout, Stdout, Write};
use std::process;
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedReceiver};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    #[default]
    Log,
    Interactive,
}

/// NB: not totally comfortable with this event bus architecture; the loose coupling is convenient
/// but it introduces non-deterministic delay, and feels overly general (over time I guess there
/// will end up being too many enum variants.
///
/// Try to keep the scope limited to actually global events; for component-to-component events,
/// consider directly coupling those pieces together.
#[derive(Debug)]
pub enum InternalEvent {
    RegisterTask(tokio::task::JoinHandle<()>),
    LogTweet(String),
    LogError(Error),
}

pub struct UI {
    stdout: Stdout,
    mode: Mode,
    events: UnboundedReceiver<InternalEvent>,
    tasks: FuturesUnordered<tokio::task::JoinHandle<()>>,
    store: Arc<Store>,
    feed_pane: Component<FeedPane>,
    bottom_bar: Component<BottomBar>,
}

impl UI {
    pub fn new(
        twitter_client: TwitterClient,
        twitter_user: &api::User,
        user_config: &UserConfig,
    ) -> Self {
        let (cols, rows) = terminal::size().unwrap();
        let (events_tx, events_rx) = mpsc::unbounded_channel();

        let store = Arc::new(Store::new(twitter_client, twitter_user, user_config));

        let feed_pane = FeedPane::new(&events_tx, &store);
        let bottom_bar = BottomBar::new(&store);

        let mut this = Self {
            stdout: stdout(),
            mode: Mode::Log,
            events: events_rx,
            tasks: FuturesUnordered::new(),
            store,
            feed_pane: Component::new(feed_pane),
            bottom_bar: Component::new(bottom_bar),
        };

        this.resize(cols, rows);
        this
    }

    pub fn initialize(&mut self) {
        self.feed_pane.component.do_load_page_of_tweets(true);
        self.set_mode(Mode::Interactive).unwrap();
    }

    // CR: just return unit and panic
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
        self.feed_pane.bounding_box = BoundingBox::new(0, 0, cols, rows - 2);
        self.bottom_bar.bounding_box = BoundingBox::new(0, rows - 1, cols, 1);
    }

    pub async fn render(&mut self) -> Result<()> {
        self.feed_pane.render_if_necessary(&mut self.stdout)?;
        self.bottom_bar.render_if_necessary(&mut self.stdout)?;

        let focus = self.feed_pane.get_cursor();
        queue!(&self.stdout, cursor::MoveTo(focus.0, focus.1))?;

        self.stdout.flush()?;
        Ok(())
    }

    pub fn log_message(&mut self, message: &str) -> Result<()> {
        self.set_mode(Mode::Log)?;
        println!("{message}\r");
        Ok(())
    }

    async fn handle_internal_event(&mut self, event: InternalEvent) {
        match event {
            InternalEvent::RegisterTask(task) => {
                self.tasks.push(task);
                self.bottom_bar
                    .component
                    .set_num_tasks_in_flight(self.tasks.len());
            }
            InternalEvent::LogTweet(tweet_id) => {
                {
                    let tweets = self.store.tweets.lock().unwrap();
                    let tweet = &tweets[&tweet_id];
                    // CR: okay, maybe handle the error here
                    fs::write("/tmp/tweet", format!("{:#?}", tweet)).unwrap();
                }

                // CR: also handle the errors here
                let mut subshell = process::Command::new("less")
                    .args(["/tmp/tweet"])
                    .spawn()
                    .unwrap();
                subshell.wait().unwrap();
            }
            InternalEvent::LogError(err) => {
                self.log_message(err.to_string().as_str()).unwrap();
            }
        }
    }

    async fn handle_terminal_event(&mut self, event: &Event) {
        match event {
            Event::Key(key_event) => {
                let handled = self.feed_pane.component.handle_key_event(key_event);
                if !handled {
                    match key_event.code {
                        KeyCode::Esc => {
                            self.set_mode(Mode::Interactive).unwrap();
                            self.feed_pane.component.invalidate();
                            self.bottom_bar.component.invalidate();
                        }
                        KeyCode::Char('q') => {
                            reset();
                            process::exit(0);
                        }
                        _ => (),
                    }
                }
            }
            Event::Resize(cols, rows) => self.resize(*cols, *rows),
            _ => (),
        }
    }

    pub async fn event_loop(&mut self) -> Result<()> {
        let mut terminal_event_stream = EventStream::new();

        loop {
            let terminal_event = terminal_event_stream.next().fuse();
            let internal_event = self.events.recv();
            let there_are_tasks = !self.tasks.is_empty();
            let task_event = self.tasks.next().fuse();

            tokio::select! {
                event = terminal_event => {
                    if let Some(Ok(event)) = event {
                        self.handle_terminal_event(&event).await;
                    }
                },
                event = internal_event => {
                    if let Some(event) = event {
                        self.handle_internal_event(event).await;
                    }
                },
                // NB: removing the precondition will cause the UI to eventually break, even if the
                // match arm handler is empty, why?
                _ = task_event, if there_are_tasks => {
                    self.bottom_bar.component.set_num_tasks_in_flight(self.tasks.len());
                }
            }

            self.render().await?
        }
    }
}

pub fn reset() {
    execute!(stdout(), LeaveAlternateScreen).unwrap();
    terminal::disable_raw_mode().unwrap()
}
