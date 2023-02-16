use std::cmp::{max, min};
use crate::twitter_client::api;
use crate::ui::{BoundingBox, Component, Input, InternalEvent, Layout, Render};
use anyhow::Result;
use crossterm::style::{self, Color};
use crossterm::{cursor, queue};
use regex::Regex;
use std::collections::HashMap;
use std::io::Stdout;
use std::sync::{Arc, Mutex};
use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;
use unicode_truncate::UnicodeTruncateStr;

pub struct FeedPane {
    events: UnboundedSender<InternalEvent>,
    tweets: Arc<Mutex<HashMap<String, api::Tweet>>>,
    tweets_reverse_chronological: Arc<Mutex<Vec<String>>>,
    tweets_scroll_offset: usize,
    tweets_selected_index: usize,
}

impl FeedPane {
    pub fn new(events: &UnboundedSender<InternalEvent>,
               tweets: &Arc<Mutex<HashMap<String, api::Tweet>>>,
               tweets_reverse_chronological: &Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            events: events.clone(),
            tweets: tweets.clone(),
            tweets_reverse_chronological: tweets_reverse_chronological.clone(),
            tweets_scroll_offset: 0,
            tweets_selected_index: 0,
        }
    }

    // CR-someday: consider changing to ID-based selection instead of absolute offset?
    fn move_selected_index(&mut self, delta: isize) {
        let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().unwrap();
        let new_index = max(0, self.tweets_selected_index as isize + delta) as usize;
        let new_index = min(new_index, tweets_reverse_chronological.len() - 1);

        if self.tweets_selected_index != new_index {
            let tweet_id = &tweets_reverse_chronological[new_index];
            self.events.send(InternalEvent::SelectTweet(tweet_id.clone())).unwrap();
        }

        self.tweets_selected_index = new_index;
        self.tweets_scroll_offset = min(self.tweets_scroll_offset, new_index);

        // CR: this updates too frequently, but we don't _know_ when a bottom re-render becomes
        // necessary we could if we kept to "last known height" as a var from render
        self.events.send(InternalEvent::FeedUpdated).unwrap();
    }
}

impl Render for FeedPane {
    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        let BoundingBox { width, height, .. } = bounding_box;
        let tweets = self.tweets.lock().unwrap();
        let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().unwrap();

        let re_newlines = Regex::new(r"[\r\n]+").unwrap();
        let str_unknown = String::from("[unknown]");

        // adjust scroll_offset to new height if necessary
        if self.tweets_selected_index - self.tweets_scroll_offset >= (height as usize) {
            self.tweets_scroll_offset = self.tweets_selected_index.saturating_sub((height - 1) as usize);
        }

        for i in 0..height {
            let tweet_idx = self.tweets_scroll_offset + (i as usize);

            if tweet_idx >= tweets_reverse_chronological.len() {
                break;
            }

            let tweet_id = &tweets_reverse_chronological[tweet_idx];
            let tweet = &tweets.get(tweet_id).unwrap();
            let mut col_offset: u16 = 0;

            let tweet_time = tweet.created_at.format("%m-%d %H:%M:%S");
            let tweet_time = format!("{tweet_time}  > ");
            queue!(stdout, cursor::MoveTo(col_offset, i))?;
            queue!(stdout, style::SetForegroundColor(Color::DarkGrey))?;
            queue!(stdout, style::Print(&tweet_time))?;
            queue!(stdout, style::ResetColor)?;
            col_offset += (tweet_time.len() + 1) as u16;

            // CR: possible to cast from String to &str?
            let tweet_author = tweet.author_username.as_ref().unwrap_or(&str_unknown);
            let tweet_author = format!("@{tweet_author}");
            queue!(stdout, cursor::MoveTo(col_offset, i))?;
            queue!(stdout, style::SetForegroundColor(Color::DarkCyan))?;
            queue!(stdout, style::Print(&tweet_author))?;
            queue!(stdout, style::ResetColor)?;
            col_offset += (&tweet_author.len() + 1) as u16;

            let formatted = re_newlines.replace_all(&tweet.text, "⏎ ");
            let (truncated, _) =
                formatted.unicode_truncate((width.saturating_sub(col_offset)) as usize);
            queue!(stdout, cursor::MoveTo(col_offset, i))?;
            queue!(stdout, style::Print(truncated))?;
        }

        Ok(())
    }
}

impl Input for FeedPane {
    fn handle_key_event(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Up => self.move_selected_index(-1),
            KeyCode::Down => self.move_selected_index(1),
            _ => ()
        }
    }

    fn get_cursor(&self) -> (u16, u16) {
        return (16, (self.tweets_selected_index - self.tweets_scroll_offset) as u16)
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
