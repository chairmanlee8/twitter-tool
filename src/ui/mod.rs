mod bottom_bar;
mod feed_pane;
mod line_buffer;
mod tweet_pane_stack;

use crate::store::Store;
use crate::twitter_client::{api, TwitterClient};
use crate::ui::bottom_bar::BottomBar;
use crate::ui::feed_pane::FeedPane;
use crate::ui::tweet_pane_stack::{TweetPaneStack, TweetPrimer};
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
use std::collections::HashMap;
use std::fs;
use std::io::{stdout, Stdout, Write};
use std::process;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    #[default]
    Log,
    Interactive,
}

#[derive(Debug, Default)]
pub enum InternalEvent {
    #[default]
    Refresh,
    FeedPaneInvalidated,
    SelectTweet(String),
    HydrateSelectedTweet(TweetPrimer),
    RegisterTask(tokio::task::JoinHandle<()>),
    LogTweet(String),
    LogError(Error),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

    fn get_cursor(&self) -> (u16, u16);
}

pub trait Input {
    fn handle_focus(&mut self);
    fn handle_key_event(&mut self, event: &KeyEvent);
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
        self.component.get_cursor()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Focus {
    FeedPane,
    TweetPaneStack,
}

impl Focus {
    pub fn next(&self) -> Self {
        match self {
            Self::FeedPane => Self::TweetPaneStack,
            Self::TweetPaneStack => Self::FeedPane,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::FeedPane => Self::TweetPaneStack,
            Self::TweetPaneStack => Self::FeedPane,
        }
    }
}

pub struct UI {
    stdout: Stdout,
    mode: Mode,
    // NB: don't abuse the event bus for component-to-component communication, this is designed for
    // truly "global" concerns like tasks and logging
    events: UnboundedReceiver<InternalEvent>,
    tasks: FuturesUnordered<tokio::task::JoinHandle<()>>,
    store: Arc<Store>,
    feed_pane: Component<FeedPane>,
    // tweet_pane_stack: Component<TweetPaneStack>,
    bottom_bar: Component<BottomBar>,
    focus: Focus,
}

impl UI {
    pub fn new(twitter_client: TwitterClient, twitter_user: &api::User) -> Self {
        let (cols, rows) = terminal::size().unwrap();
        let (events_tx, events_rx) = mpsc::unbounded_channel();

        let store = Arc::new(Store::new(twitter_client, twitter_user));

        let feed_pane = FeedPane::new(&events_tx, &store);
        // let tweet_pane = TweetPaneStack::new(&tx, &tweets);
        let bottom_bar = BottomBar::new(&store);

        let mut this = Self {
            stdout: stdout(),
            mode: Mode::Log,
            events: events_rx,
            tasks: FuturesUnordered::new(),
            store,
            feed_pane: Component::new(feed_pane),
            // tweet_pane_stack: Component::new(tweet_pane),
            bottom_bar: Component::new(bottom_bar),
            focus: Focus::FeedPane,
        };

        this.resize(cols, rows);
        this
    }

    pub fn initialize(&mut self) {
        self.feed_pane.component.do_load_page_of_tweets(true);
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
        // self.tweet_pane_stack.bounding_box =
        //     BoundingBox::new(half_width + 1, 0, half_width, rows - 2);
        self.bottom_bar.bounding_box = BoundingBox::new(0, rows - 1, cols, 1);
    }

    pub async fn render(&mut self) -> Result<()> {
        self.set_mode(Mode::Interactive)?;

        self.feed_pane.render_if_necessary(&mut self.stdout)?;
        // self.tweet_pane_stack
        //     .render_if_necessary(&mut self.stdout)?;
        self.bottom_bar.render_if_necessary(&mut self.stdout)?;

        let focus = match self.focus {
            Focus::FeedPane => self.feed_pane.get_cursor(),
            // Focus::TweetPaneStack => self.tweet_pane_stack.get_cursor(),
            _ => (0, 0),
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

    async fn handle_internal_event(&mut self, event: InternalEvent) {
        match event {
            InternalEvent::Refresh => (),
            InternalEvent::FeedPaneInvalidated => {
                self.feed_pane.should_render = true;
                self.bottom_bar.should_render = true;
            }
            InternalEvent::SelectTweet(tweet_id) => {
                let tweet_primer = TweetPrimer::new(&tweet_id);
                // self.tweet_pane_stack
                //     .component
                //     .open_tweet_pane(&tweet_primer);
                // self.tweet_pane_stack.should_render = true;
            }
            InternalEvent::HydrateSelectedTweet(_) => (),
            InternalEvent::RegisterTask(task) => {
                self.tasks.push(task);
                self.bottom_bar
                    .component
                    .set_num_tasks_in_flight(self.tasks.len());
                self.bottom_bar.should_render = true;
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
            Event::Key(key_event) => match key_event.code {
                KeyCode::Esc => {
                    self.feed_pane.should_render = true;
                    // self.tweet_pane_stack.should_render = true;
                    self.bottom_bar.should_render = true;
                }
                KeyCode::Tab => {
                    self.focus = self.focus.next();
                    // TODO: maybe factor
                    match self.focus {
                        Focus::FeedPane => self.feed_pane.component.handle_focus(),
                        // Focus::TweetPaneStack => self.tweet_pane_stack.component.handle_focus(),
                        _ => (),
                    };
                }
                KeyCode::Char('h') => {
                    self.log_message("hello").unwrap();
                }
                KeyCode::Char('q') => {
                    reset();
                    process::exit(0);
                }
                _ => match self.focus {
                    Focus::FeedPane => self.feed_pane.component.handle_key_event(key_event),
                    // Focus::TweetPaneStack => {
                    //     self.tweet_pane_stack.component.handle_key_event(key_event)
                    // }
                    _ => (),
                },
            },
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
                    self.bottom_bar.should_render = true;
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
