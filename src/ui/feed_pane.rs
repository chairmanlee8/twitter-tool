use crate::store::Store;
use crate::twitter_client::api;
use crate::ui::{BoundingBox, Input, InternalEvent, Render};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use crossterm::style::{self, Color};
use crossterm::{cursor, queue};
use regex::Regex;
use std::cmp::{max, min};
use std::collections::HashMap;
use std::io::Stdout;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

pub struct FeedPane {
    events: UnboundedSender<InternalEvent>,
    store: Arc<Store>,
    tweets_selected_index: usize,
    scroll_offset: usize,
    cursor_position: (u16, u16),
    last_known_height: u16,
}

impl FeedPane {
    pub fn new(events: &UnboundedSender<InternalEvent>, store: &Arc<Store>) -> Self {
        Self {
            events: events.clone(),
            store: store.clone(),
            tweets_selected_index: 0,
            scroll_offset: 0,
            cursor_position: (0, 0),
            last_known_height: 0,
        }
    }

    // CR-someday: consider changing to ID-based selection instead of absolute offset?
    fn move_selected_index(&mut self, delta: isize) {
        let tweets_reverse_chronological = self.store.tweets_reverse_chronological.lock().unwrap();
        let new_index = max(0, self.tweets_selected_index as isize + delta) as usize;
        let new_index = min(
            new_index,
            tweets_reverse_chronological.len().saturating_sub(1),
        );

        if self.tweets_selected_index != new_index {
            self.tweets_selected_index = new_index;
            let tweet_id = &tweets_reverse_chronological[new_index];
            self.events
                .send(InternalEvent::SelectTweet(tweet_id.clone()))
                .unwrap();
        }

        if new_index < self.scroll_offset {
            self.scroll_offset = new_index;
            self.events
                .send(InternalEvent::FeedPaneInvalidated)
                .unwrap();
        } else if new_index >= self.scroll_offset + (self.last_known_height as usize) {
            self.events
                .send(InternalEvent::FeedPaneInvalidated)
                .unwrap();
        } else {
            // Feed pane still valid, just update cursor
            if delta.is_positive() {
                self.cursor_position.1 += delta as u16;
            } else {
                self.cursor_position.1 = self.cursor_position.1.saturating_sub(delta.abs() as u16);
            }
        }
    }

    pub fn do_load_page_of_tweets(&self, restart: bool) {
        let events = self.events.clone();
        let store = self.store.clone();

        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            match store.load_tweets_reverse_chronological(restart).await {
                Ok(()) => events.send(InternalEvent::FeedPaneInvalidated).unwrap(),
                Err(error) => events.send(InternalEvent::LogError(error)).unwrap(),
            }
        });

        self.events.send(InternalEvent::RegisterTask(task)).unwrap();
    }
}

impl Render for FeedPane {
    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        let BoundingBox {
            left,
            top,
            width,
            height,
        } = bounding_box;

        let tweets = self.store.tweets.lock().unwrap();
        let tweets_reverse_chronological = self.store.tweets_reverse_chronological.lock().unwrap();

        let re_newlines = Regex::new(r"[\r\n]+").unwrap();
        let str_unknown = String::from("[unknown]");
        let str_clear = " ".repeat(width as usize);

        self.last_known_height = height;

        // adjust scroll_offset to new height if necessary
        if self.tweets_selected_index - self.scroll_offset >= (height as usize) {
            self.scroll_offset = self
                .tweets_selected_index
                .saturating_sub((height - 1) as usize);
        }

        self.cursor_position = (
            left + 16,
            top + (self.tweets_selected_index - self.scroll_offset) as u16,
        );

        for i in 0..height {
            let tweet_idx = self.scroll_offset + (i as usize);

            if tweet_idx >= tweets_reverse_chronological.len() {
                break;
            }

            let tweet_id = &tweets_reverse_chronological[tweet_idx];
            let tweet = &tweets.get(tweet_id).unwrap();
            let mut col_offset: u16 = left;

            // Clear the line
            queue!(stdout, cursor::MoveTo(col_offset, top + i))?;
            queue!(stdout, style::Print(&str_clear))?;

            let tweet_time = tweet.created_at.format("%m-%d %H:%M:%S");
            let tweet_time = format!("{tweet_time}  >  ");
            queue!(stdout, cursor::MoveTo(col_offset, top + i))?;
            queue!(stdout, style::SetForegroundColor(Color::DarkGrey))?;
            queue!(stdout, style::Print(&tweet_time))?;
            queue!(stdout, style::ResetColor)?;
            col_offset += tweet_time.len() as u16;

            let tweet_author = tweet.author_username.as_ref().unwrap_or(&str_unknown);
            let tweet_author = format!("@{tweet_author} ");
            queue!(stdout, style::SetForegroundColor(Color::DarkCyan))?;
            queue!(stdout, style::Print(&tweet_author))?;
            queue!(stdout, style::ResetColor)?;
            col_offset += tweet_author.len() as u16;

            let formatted = re_newlines.replace_all(&tweet.text, "⏎ ");
            let remaining_length = width.saturating_sub(col_offset) as usize;
            let lines = textwrap::wrap(&formatted, remaining_length);
            if lines.len() == 1 {
                queue!(stdout, style::Print(&lines[0]))?;
            } else if lines.len() > 1 {
                // Rewrap lines to accommodate ellipsis (…), which may knock out a word
                let remaining_length = width.saturating_sub(col_offset + 1) as usize;
                let lines = textwrap::wrap(&formatted, remaining_length);
                queue!(stdout, style::Print(&lines[0]))?;
                queue!(stdout, style::Print("…"))?;
            }
        }

        Ok(())
    }

    fn get_cursor(&self) -> (u16, u16) {
        return self.cursor_position;
    }
}

impl Input for FeedPane {
    fn handle_focus(&mut self) {
        ()
    }

    fn handle_key_event(&mut self, event: &KeyEvent) {
        match event.code {
            KeyCode::Up => self.move_selected_index(-1),
            KeyCode::Down => self.move_selected_index(1),
            KeyCode::Char('i') => {
                let feed = self.store.tweets_reverse_chronological.lock().unwrap();
                let selected_id = &feed[self.tweets_selected_index];
                self.events
                    .send(InternalEvent::LogTweet(selected_id.clone()))
                    .unwrap();
            }
            KeyCode::Char('n') => self.do_load_page_of_tweets(false),
            _ => (),
        }
    }
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
