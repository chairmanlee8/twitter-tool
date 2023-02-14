mod bottom_bar;
mod tweet_pane;
mod tweets_pane;

use crate::api::Tweet;
use crate::ui::bottom_bar::render_bottom_bar;
use crate::ui::tweets_pane::render_tweets_pane;
use crossterm::cursor;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size, Clear, ClearType};
use crossterm::{
    execute, queue,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    Result,
};
use std::cmp::{max, min};
use std::fs;
use std::process;
use std::io::{stdout, Write};
use crate::ui::tweet_pane::render_tweet_pane;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum UIMode {
    Log,
    Interactive,
}

pub struct Context {
    pub screen_cols: u16,
    pub screen_rows: u16,
}

// CR: should the UI really own state (tweets) here?  Seems like tweets should have a separate
// state machine more connected to TwitterClient
pub struct UI {
    mode: UIMode,
    context: Context,
    tweets: Vec<Tweet>,
    tweets_view_offset: usize,
    tweets_selected_index: usize,
    tweet_pane_width: u16
}

impl UI {
    pub fn new() -> Self {
        let (cols, rows) = size().unwrap();

        Self {
            mode: UIMode::Log,
            context: Context {
                screen_cols: cols,
                screen_rows: rows,
            },
            tweets: Vec::new(),
            tweets_view_offset: 0,
            tweets_selected_index: 0,
            tweet_pane_width: 80
        }
    }

    fn set_mode(&mut self, mode: UIMode) -> Result<()> {
        let prev_mode = self.mode;
        self.mode = mode;

        // CR: no automatic deref?
        if prev_mode == UIMode::Log && mode == UIMode::Interactive {
            execute!(stdout(), EnterAlternateScreen)?;
            enable_raw_mode()?;
        } else if prev_mode == UIMode::Interactive && mode == UIMode::Log {
            execute!(stdout(), LeaveAlternateScreen)?;
            enable_raw_mode()?;
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

    pub fn set_tweets(&mut self, tweets: &Vec<Tweet>) {
        // CR: seems wrong to clone, maybe now we need a weak ref
        self.tweets = tweets.clone()
    }

    pub fn move_selected_index(&mut self, delta: isize) -> Result<()> {
        let new_index = max(0, self.tweets_selected_index as isize + delta) as usize;
        let new_index = min(new_index, self.tweets.len() - 1);
        let view_top = self.tweets_view_offset;
        let view_height = (self.context.screen_rows - 3) as usize;
        let view_bottom = self.tweets_view_offset + view_height;

        self.tweets_selected_index = new_index;

        if new_index < view_top {
            self.tweets_view_offset = new_index;
            self.show_tweets()
        } else if new_index > view_bottom {
            self.tweets_view_offset = max(0, new_index - view_height);
            self.show_tweets()
        } else {
            // CR: this is confusing re the above two conditions, consider a refactor
            render_tweet_pane(&self.context, self.tweet_pane_width, &self.tweets[self.tweets_selected_index])?;
            render_bottom_bar(&self.context, &self.tweets, self.tweets_selected_index)?;
            execute!(
                stdout(),
                cursor::MoveTo(
                    16,
                    (self.tweets_selected_index - self.tweets_view_offset) as u16
                )
            )
        }
    }

    // CR: switch to render_tweets (show_tweets then just sets state)
    pub fn show_tweets(&mut self) -> Result<()> {
        self.set_mode(UIMode::Interactive)?;

        queue!(stdout(), Clear(ClearType::All))?;

        render_tweets_pane(&self.context, self.context.screen_cols - self.tweet_pane_width - 2, &self.tweets, self.tweets_view_offset)?;
        render_tweet_pane(&self.context, self.tweet_pane_width, &self.tweets[self.tweets_selected_index])?;
        render_bottom_bar(&self.context, &self.tweets, self.tweets_selected_index)?;

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

    // CR-someday: maybe consider a [less] instead
    pub fn log_selected_tweet(&mut self) -> Result<()> {
        fs::write("/tmp/tweet", format!("{:#?}", self.tweets[self.tweets_selected_index]))?;
        let mut subshell = process::Command::new("less").args(["/tmp/tweet"]).spawn()?;
        subshell.wait()?;
        Ok(())
    }

    pub fn log_message(&mut self, message: &str) -> Result<()> {
        self.set_mode(UIMode::Log)?;
        println!("{message}");
        Ok(())
    }
}

pub fn reset() {
    execute!(stdout(), LeaveAlternateScreen).unwrap();
    disable_raw_mode().unwrap()
}
