use crate::store::Store;
use crate::ui::InternalEvent;
use crate::ui_framework::scroll_buffer::{ScrollBuffer, TextSegment};
use crate::ui_framework::{bounding_box::BoundingBox, Input, Render};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use crossterm::style::{self, Color, Colors};
use crossterm::{cursor, queue};
use regex::Regex;
use std::cmp::{max, min};
use std::io::Stdout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use textwrap::core::display_width;
use tokio::sync::mpsc::UnboundedSender;

pub struct FeedPane {
    events: UnboundedSender<InternalEvent>,
    store: Arc<Store>,
    scroll_buffer: ScrollBuffer,
    should_update_scroll_buffer: Arc<AtomicBool>,
    display_width: usize,
    tweets_selected_index: usize,
}

impl FeedPane {
    pub fn new(events: &UnboundedSender<InternalEvent>, store: &Arc<Store>) -> Self {
        Self {
            events: events.clone(),
            store: store.clone(),
            scroll_buffer: ScrollBuffer::new(),
            should_update_scroll_buffer: Arc::new(AtomicBool::new(true)),
            display_width: 0,
            tweets_selected_index: 0,
        }
    }

    fn update_scroll_buffer(&mut self) {
        self.scroll_buffer.clear();

        let tweets = self.store.tweets.lock().unwrap();
        let tweets_reverse_chronological = self.store.tweets_reverse_chronological.lock().unwrap();

        let re_newlines = Regex::new(r"[\r\n]+").unwrap();
        let str_unknown = String::from("[unknown]");

        for tweet_id in tweets_reverse_chronological.iter() {
            let tweet = &tweets.get(tweet_id).unwrap();
            let mut segments: Vec<TextSegment> = Vec::new();

            let tweet_time = tweet.created_at.format("%m-%d %H:%M:%S");
            let tweet_time = format!("{tweet_time}  >  ");
            segments.push(TextSegment::color(
                &tweet_time,
                Colors::new(Color::DarkGrey, Color::Reset),
            ));

            let tweet_author = tweet.author_username.as_ref().unwrap_or(&str_unknown);
            let tweet_author = format!("@{tweet_author} ");
            segments.push(TextSegment::color(
                &tweet_author,
                Colors::new(Color::DarkCyan, Color::Reset),
            ));

            let formatted = re_newlines.replace_all(&tweet.text, "⏎ ");
            let used_length = tweet_time.len() + tweet_author.len();
            let remaining_length = self.display_width.saturating_sub(used_length);
            let lines = textwrap::wrap(&formatted, remaining_length);
            if lines.len() == 1 {
                segments.push(TextSegment::plain(&lines[0]));
            } else if lines.len() > 1 {
                // Rewrap lines to accommodate ellipsis (…), which may knock out a word
                let remaining_length = remaining_length.saturating_sub(1) as usize;
                let lines = textwrap::wrap(&formatted, remaining_length);
                segments.push(TextSegment::plain(&lines[0]));
                segments.push(TextSegment::plain("…"));
            }

            self.scroll_buffer.push(segments);
        }

        self.scroll_buffer.move_cursor(0);
        self.should_update_scroll_buffer
            .store(false, Ordering::SeqCst);
    }

    pub fn do_load_page_of_tweets(&self, restart: bool) {
        let events = self.events.clone();
        let store = self.store.clone();
        let should_update_scroll_buffer = self.should_update_scroll_buffer.clone();

        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            match store.load_tweets_reverse_chronological(restart).await {
                Ok(()) => should_update_scroll_buffer.store(true, Ordering::SeqCst),
                Err(error) => events.send(InternalEvent::LogError(error)).unwrap(),
            }
        });

        self.events.send(InternalEvent::RegisterTask(task)).unwrap();
    }
}

impl Render for FeedPane {
    fn should_render(&self) -> bool {
        self.should_update_scroll_buffer.load(Ordering::SeqCst)
            || self.scroll_buffer.should_render()
    }

    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        // CR-someday: does using SeqCst have a performance impact?  Frankly, we already use Mutex
        // in the render loop, so I'm not sure it matters.
        let width = bounding_box.width as usize;

        if self.should_update_scroll_buffer.load(Ordering::SeqCst) || self.display_width != width {
            self.display_width = width;
            self.update_scroll_buffer();
        }

        self.scroll_buffer.render(stdout, bounding_box)?;

        Ok(())
    }

    fn get_cursor(&self) -> (u16, u16) {
        let cursor = self.scroll_buffer.get_cursor();
        // CR: hmm
        (16, cursor.1)
    }
}

impl Input for FeedPane {
    fn handle_focus(&mut self) {
        self.scroll_buffer.handle_focus()
    }

    fn handle_key_event(&mut self, event: &KeyEvent) {
        match event.code {
            KeyCode::Char('i') => {
                // CR: factor
                let feed = self.store.tweets_reverse_chronological.lock().unwrap();
                let selected_id = &feed[self.tweets_selected_index];
                self.events
                    .send(InternalEvent::LogTweet(selected_id.clone()))
                    .unwrap();
            }
            KeyCode::Char('n') => self.do_load_page_of_tweets(false),
            _ => self.scroll_buffer.handle_key_event(event),
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
