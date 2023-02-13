use std::cmp::{max, min};
use crate::api::Tweet;
use crossterm::event::{read, Event, KeyCode, KeyModifiers};
use crossterm::style::{self, Stylize, Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor};
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
    cols: u16,
    rows: u16,
    tweets: Vec<Tweet>,
    tweets_view_offset: usize,
    tweets_selected_index: usize
}

impl UI {
    pub fn new() -> Self {
        let (cols, rows) = size().unwrap();

        Self {
            mode: UIMode::Log,
            cols,
            rows,
            tweets: Vec::new(),
            tweets_view_offset: 0,
            tweets_selected_index: 0
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

    pub fn set_tweets(&mut self, tweets: Vec<Tweet>) {
        self.tweets = tweets
    }

    pub fn set_selected_index(&mut self, new_index: usize) -> Result<()> {
        // CR: clamp?
        let new_index = max(0, min(new_index, self.tweets.len() - 1));
        let view_top = self.tweets_view_offset;
        let view_height = (self.rows - 3) as usize;
        let view_bottom = self.tweets_view_offset + view_height;

        self.tweets_selected_index = new_index;

        if new_index < view_top {
            self.tweets_view_offset = new_index;
            self.show_tweets()
        } else if new_index > view_bottom {
            self.tweets_view_offset = max(0, new_index - view_height);
            self.show_tweets()
        } else {
            self.render_status_bar()?;
            execute!(stdout(), cursor::MoveTo(16, (self.tweets_selected_index - self.tweets_view_offset) as u16))
        }
    }

    // CR: switch to render_tweets (show_tweets then just sets state)
    pub fn show_tweets(&mut self) -> Result<()> {
        self.set_mode(UIMode::Interactive)?;

        let (cols, rows) = size()?;
        let mut stdout = stdout();

        queue!(stdout, Clear(ClearType::All))?;

        let re_newlines = Regex::new("[\r\n]+").unwrap();
        let str_unknown = String::from("[unknown]");

        for i in 0..(rows - 2) {
            if i > self.tweets.len() as u16 {
                break;
            }

            let tweet = &self.tweets[self.tweets_view_offset + (i as usize)];
            let mut col_offset: u16 = 0;

            let tweet_time = tweet.created_at.format("%m-%d %H:%M:%S");
            let tweet_time = format!("{tweet_time}  > ");
            queue!(stdout, cursor::MoveTo(col_offset, i))?;
            queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
            queue!(stdout, style::Print(&tweet_time))?;
            queue!(stdout, ResetColor)?;
            col_offset += (tweet_time.len() + 1) as u16;

            // CR: possible to cast from String to &str?
            let tweet_author = tweet.author_username.as_ref().unwrap_or(&str_unknown);
            let tweet_author = format!("@{tweet_author}");
            queue!(stdout, cursor::MoveTo(col_offset, i))?;
            queue!(stdout, SetForegroundColor(Color::DarkCyan))?;
            queue!(stdout, style::Print(&tweet_author))?;
            queue!(stdout, ResetColor)?;
            col_offset += (&tweet_author.len() + 1) as u16;

            let formatted = re_newlines.replace(&tweet.text, "⏎ ");
            let (truncated, _) = formatted.unicode_truncate((cols.saturating_sub(col_offset)) as usize);
            queue!(stdout, cursor::MoveTo(col_offset, i))?;
            queue!(stdout, style::Print(truncated))?;
        }

        self.render_status_bar()?;

        queue!(stdout, cursor::MoveTo(16, (self.tweets_selected_index - self.tweets_view_offset) as u16))?;
        stdout.flush()?;
        Ok(())
    }

    fn render_status_bar(&self) -> Result<()> {
        let (cols, rows) = size()?;
        let mut stdout = stdout();

        queue!(stdout, cursor::MoveTo(0, rows - 1))?;
        queue!(stdout, SetForegroundColor(Color::Black))?;
        queue!(stdout, SetBackgroundColor(Color::White))?;
        queue!(stdout, Print(format!("{}/{} tweets", self.tweets_selected_index, self.tweets.len())))?;
        queue!(stdout, ResetColor)?;

        stdout.flush()?;
        Ok(())
    }

    // CR-someday: maybe consider a [less] instead
    fn log_tweet(&mut self, index: usize) -> Result<()> {
        self.set_mode(UIMode::Log)?;
        println!("{:?}", self.tweets[self.tweets_selected_index]);
        Ok(())
    }

    pub fn process_events_until_quit(&mut self) -> Result<()> {
        loop {
            match read()? {
                Event::Key(key_event) => {
                    match key_event.code {
                        KeyCode::Esc => {
                            self.show_tweets();
                        },
                        KeyCode::Up => {
                            self.set_selected_index(self.tweets_selected_index.saturating_sub(1));
                        },
                        KeyCode::Down => {
                            self.set_selected_index(self.tweets_selected_index + 1);
                        },
                        KeyCode::Char('i') => {
                            self.log_tweet(self.tweets_selected_index);
                        },
                        KeyCode::Char('q') => {
                            reset();
                            std::process::exit(0);
                        },
                        _ => ()
                    }
                },
                Event::Resize(cols, rows) => {
                    self.cols = cols;
                    self.rows = rows;
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
