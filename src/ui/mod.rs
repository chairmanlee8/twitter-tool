mod bottom_bar;
mod tweets_pane;

use crate::api::Tweet;
use crate::ui::bottom_bar::render_bottom_bar;
use crate::ui::tweets_pane::render_tweets_pane;
use crossterm::cursor;
use crossterm::event::{read, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size, Clear, ClearType};
use crossterm::{
    execute, queue,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    Result,
};
use std::cmp::{max, min};
use std::io::{stdout, Write};

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

        render_tweets_pane(&self.context, &self.tweets, self.tweets_view_offset)?;
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
    fn log_tweet(&mut self, index: usize) -> Result<()> {
        self.set_mode(UIMode::Log)?;
        println!("{:?}", self.tweets[index]);
        Ok(())
    }

    pub fn process_events_until_quit(&mut self) -> Result<()> {
        loop {
            match read()? {
                Event::Key(key_event) => match key_event.code {
                    KeyCode::Esc => {
                        self.show_tweets()?;
                    }
                    KeyCode::Up => {
                        self.set_selected_index(self.tweets_selected_index.saturating_sub(1))?;
                    }
                    KeyCode::Down => {
                        self.set_selected_index(self.tweets_selected_index + 1)?;
                    }
                    KeyCode::Char('i') => {
                        self.log_tweet(self.tweets_selected_index)?;
                    }
                    KeyCode::Char('q') => {
                        reset();
                        std::process::exit(0);
                    }
                    _ => (),
                },
                Event::Resize(cols, rows) => {
                    self.context = Context {
                        screen_cols: cols,
                        screen_rows: rows,
                    };
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex() {
        let re_newlines = Regex::new(r"[\r\n]+").unwrap();
        let str = "Detected new closed trade\n\nTrader: @Burgerinnn\nSymbol: $ETH\nPosition: short ↘\u{fe0f}\nEntry: 1 500.6\nExit: 1 498.2\nProfit: 3 994\nLeverage: 10x\n\nEntry, take profit, stats, leaderboard can be found at https://t.co/EFjrCz4DgD";
        let result = re_newlines.replace_all(str, "⏎ ");
        let expected = "Detected new closed trade⏎ Trader: @Burgerinnn⏎ Symbol: $ETH⏎ Position: short ↘\u{fe0f}⏎ Entry: 1 500.6⏎ Exit: 1 498.2⏎ Profit: 3 994⏎ Leverage: 10x⏎ Entry, take profit, stats, leaderboard can be found at https://t.co/EFjrCz4DgD";
        assert_eq!(result, expected);
    }
}
