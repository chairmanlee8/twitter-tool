use crate::api::Tweet;
use crossterm::event::{read, Event, KeyCode, KeyModifiers};
use crossterm::style::{self, Stylize};
use crossterm::terminal::{size, Clear, ClearType, enable_raw_mode, disable_raw_mode};
use crossterm::{cursor, QueueableCommand};
use crossterm::{
    execute, queue,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    Result,
};
use regex::Regex;
use std::error::Error;
use std::io::{stdout, Write};
use unicode_truncate::UnicodeTruncateStr;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum UIMode {
    Log,
    Interactive,
}

pub struct UI {
    mode: UIMode,
    tweets: Vec<Tweet>,
    tweets_view_offset: usize,
}

impl UI {
    pub fn new() -> Self {
        Self {
            mode: UIMode::Log,
            tweets: Vec::new(),
            tweets_view_offset: 0,
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
            disable_raw_mode()?;
        }

        Ok(())
    }

    pub fn set_tweets(&mut self, tweets: Vec<Tweet>) {
        self.tweets = tweets
    }

    pub fn show_tweets(&mut self) -> Result<()> {
        self.set_mode(UIMode::Interactive)?;

        let (cols, rows) = size()?;
        let mut stdout = stdout();

        execute!(stdout, Clear(ClearType::All))?;

        let re_newlines = Regex::new("[\r\n]+").unwrap();
        let str_unknown = String::from("@[unknown]");

        for i in 0..(rows - 1) {
            if i > self.tweets.len() as u16 {
                break;
            }

            let tweet = &self.tweets[self.tweets_view_offset + (i as usize)];
            // CR: possible to cast from String to &str?
            let tweet_author = tweet.author_username.as_ref().unwrap_or(&str_unknown);
            let (truncated, _) = tweet_author.unicode_truncate(20);
            queue!(stdout, cursor::MoveTo(0, i))?;
            queue!(stdout, style::Print(truncated))?;

            let formatted = re_newlines.replace(&tweet.text, "⏎ ");
            let (truncated, _) = formatted.unicode_truncate((cols.saturating_sub(22)) as usize);
            queue!(stdout, cursor::MoveTo(22, i))?;
            queue!(stdout, style::Print(truncated))?;
        }

        queue!(stdout, cursor::MoveTo(0, 0))?;
        stdout.flush()?;
        Ok(())
    }

    pub fn process_events_until_quit(&self) -> Result<()> {
        loop {
            match read()? {
                Event::Key(key_event) => {
                    match key_event.code {
                        KeyCode::Char('q') => {
                            reset();
                            std::process::exit(0);
                        },
                        _ => ()
                    }
                },
                _ => (),
            }
        }
        Ok(())
    }
}

pub fn reset() {
    execute!(stdout(), LeaveAlternateScreen).unwrap();
    disable_raw_mode().unwrap()
}
