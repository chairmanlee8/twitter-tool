use crate::store::Store;
use crate::twitter_client::api;
use crate::ui::tweet_pane::TweetPane;
use crate::ui::InternalEvent;
use crate::ui_framework::scroll_buffer::{ScrollBuffer, TextSegment};
use crate::ui_framework::{bounding_box::BoundingBox, Component, Input, Render};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use crossterm::style::{Color, Colors};
use regex::Regex;
use std::borrow::Cow;
use std::io::Stdout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Focus {
    FeedPane,
    TweetPaneStack,
}

pub struct FeedPane {
    events: UnboundedSender<InternalEvent>,
    store: Arc<Store>,
    scroll_buffer: ScrollBuffer,
    should_update_scroll_buffer: Arc<AtomicBool>,
    display_width: usize,
    focus: Focus,
    tweet_selected_id: String,
    tweet_pane: Component<TweetPane>,
}

impl FeedPane {
    pub fn new(events: &UnboundedSender<InternalEvent>, store: &Arc<Store>) -> Self {
        let tweet_selected_id = String::from("0");
        let tweet_pane = Component::new(TweetPane::new(events, store, &tweet_selected_id));

        Self {
            events: events.clone(),
            store: store.clone(),
            scroll_buffer: ScrollBuffer::new(),
            should_update_scroll_buffer: Arc::new(AtomicBool::new(true)),
            display_width: 0,
            focus: Focus::FeedPane,
            tweet_selected_id,
            tweet_pane,
        }
    }

    pub fn get_selected_tweet_id(&self) -> Option<String> {
        let line_no = self.scroll_buffer.get_cursor_line();
        {
            let feed = self.store.tweets_reverse_chronological.lock().unwrap();
            if let Some(tweet_id) = feed.get(line_no as usize) {
                return Some(tweet_id.clone());
            }
        }
        None
    }

    fn update_scroll_buffer(&mut self) {
        self.scroll_buffer.clear();

        let tweets = self.store.tweets.lock().unwrap();
        let tweets_reverse_chronological = self.store.tweets_reverse_chronological.lock().unwrap();
        let user_config = self.store.user_config.lock().unwrap();

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
            let is_starred = user_config.is_starred(&tweet.author_id);
            segments.push(TextSegment::color(
                &tweet_author,
                if is_starred {
                    Colors::new(Color::Yellow, Color::Reset)
                } else {
                    Colors::new(Color::DarkCyan, Color::Reset)
                },
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

        let y = self.scroll_buffer.get_cursor().1;
        self.scroll_buffer.move_cursor_to(16, y as usize);
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

    fn do_toggle_selected_tweet_starred(&mut self) {
        if let Some(tweet_id) = self.get_selected_tweet_id() {
            if let Some(tweet) = self.store.tweets.lock().unwrap().get(&tweet_id) {
                {
                    let mut user_config = self.store.user_config.lock().unwrap();
                    let tweet_author = tweet.author("[unknown]");

                    if user_config.is_starred(&tweet.author_id) {
                        user_config.unstar_account(&tweet_author);
                    } else {
                        user_config.star_account(&tweet_author);
                    }
                }

                // CR-soon: the change shouldn't commit until after the config is saved
                match self.store.save_user_config() {
                    Ok(()) => self
                        .should_update_scroll_buffer
                        .store(true, Ordering::SeqCst),
                    Err(err) => self.events.send(InternalEvent::LogError(err)).unwrap(),
                }
            }
        }
    }

    pub fn log_selected_tweet(&self) {
        self.events
            .send(InternalEvent::LogTweet(self.tweet_selected_id.clone()))
            .unwrap();
    }
}

impl Render for FeedPane {
    fn should_render(&self) -> bool {
        self.should_update_scroll_buffer.load(Ordering::SeqCst)
            || self.scroll_buffer.should_render()
            || self.tweet_pane.component.should_render()
    }

    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        // CR-someday: does using SeqCst have a performance impact?  Frankly, we already use Mutex
        // in the render loop, so I'm not sure it matters.
        let BoundingBox { left, width, .. } = bounding_box;
        let half_width = ((width as usize) / 2).saturating_sub(1);

        if self.should_update_scroll_buffer.load(Ordering::SeqCst)
            || self.display_width != half_width as usize
        {
            self.display_width = half_width;
            self.update_scroll_buffer();
        }

        self.scroll_buffer.render(
            stdout,
            BoundingBox {
                width: half_width as u16,
                ..bounding_box
            },
        )?;

        self.tweet_pane.bounding_box = BoundingBox {
            left: left + (half_width as u16) + 1,
            width: half_width.saturating_sub(2) as u16,
            ..bounding_box
        };
        self.tweet_pane.render_if_necessary(stdout)?;

        Ok(())
    }

    fn get_cursor(&self) -> (u16, u16) {
        match self.focus {
            Focus::FeedPane => self.scroll_buffer.get_cursor(),
            Focus::TweetPaneStack => self.tweet_pane.get_cursor(),
        }
    }
}

impl Input for FeedPane {
    fn handle_focus(&mut self) {
        match self.focus {
            Focus::FeedPane => self.scroll_buffer.handle_focus(),
            Focus::TweetPaneStack => self.tweet_pane.component.handle_focus(),
        }
    }

    fn handle_key_event(&mut self, event: &KeyEvent) {
        match event.code {
            KeyCode::Tab => {
                let next_focus = match self.focus {
                    Focus::FeedPane => Focus::TweetPaneStack,
                    Focus::TweetPaneStack => Focus::FeedPane,
                };
                self.focus = next_focus;
                self.handle_focus();
            }
            _ => match self.focus {
                Focus::FeedPane => match event.code {
                    KeyCode::Char('i') => self.log_selected_tweet(),
                    KeyCode::Char('n') => self.do_load_page_of_tweets(false),
                    KeyCode::Char('s') => self.do_toggle_selected_tweet_starred(),
                    _ => {
                        self.scroll_buffer.handle_key_event(event);

                        if let Some(tweet_id) = self.get_selected_tweet_id() {
                            self.tweet_selected_id = tweet_id.clone();
                            self.tweet_pane.component.set_tweet_id(&tweet_id);
                        }
                    }
                },
                Focus::TweetPaneStack => self.tweet_pane.component.handle_key_event(event),
            },
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
