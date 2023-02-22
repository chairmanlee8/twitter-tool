use crate::store::Store;
use crate::twitter_client::api;
use crate::ui::InternalEvent;
use crate::ui_framework::bounding_box::BoundingBox;
use crate::ui_framework::scroll_buffer::{ScrollBuffer, TextSegment};
use crate::ui_framework::{Input, Render};
use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{KeyCode, KeyEvent};
use crossterm::queue;
use crossterm::style::{self, Color, Colors};
use regex::Regex;
use std::collections::HashMap;
use std::io::{Stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

// TODO: so there's now two types of focus, TAB focus and ARROW focus...
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
enum Focus {
    InReplyTo(usize),
    #[default]
    Tweet,
    Reply(usize),
    Quote,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TweetDetails {
    pub in_reply_to_ids: Option<Vec<String>>,
    pub tweet_id: String,
    pub quote_id: Option<(QuoteType, String)>,
    pub reply_ids: Option<Vec<String>>,
}

impl TweetDetails {
    pub fn new(tweet_id: &str) -> Self {
        Self {
            in_reply_to_ids: None,
            tweet_id: tweet_id.to_string(),
            quote_id: None,
            reply_ids: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum QuoteType {
    #[default]
    Retweet,
    QuoteTweet,
}

#[derive(Debug, Clone)]
pub struct TweetPane {
    events: UnboundedSender<InternalEvent>,
    store: Arc<Store>,
    tweet_details: Arc<Mutex<TweetDetails>>,
    scroll_buffer: ScrollBuffer,
    should_update_scroll_buffer: Arc<AtomicBool>,
    display_width: usize,
    focus: Focus,
    focus_map: HashMap<Focus, (usize, usize)>,
}

impl TweetPane {
    pub fn new(
        events: &UnboundedSender<InternalEvent>,
        store: &Arc<Store>,
        tweet_id: &str,
    ) -> Self {
        Self {
            events: events.clone(),
            store: store.clone(),
            tweet_details: Arc::new(Mutex::new(TweetDetails::new(tweet_id))),
            scroll_buffer: ScrollBuffer::new(),
            should_update_scroll_buffer: Arc::new(AtomicBool::new(true)),
            display_width: 0,
            focus: Focus::Tweet,
            focus_map: HashMap::new(),
        }
    }

    pub fn set_tweet_id(&mut self, tweet_id: &String) {
        let mut tweet_details = self.tweet_details.lock().unwrap();
        tweet_details.tweet_id = tweet_id.clone();
        self.should_update_scroll_buffer
            .store(true, Ordering::Relaxed);
    }

    fn set_focus(&mut self, focus: &Focus) {
        let desired = self.focus_map.get(&focus).map(|cur| (focus, cur));
        let default = self
            .focus_map
            .get(&Focus::Tweet)
            .map(|cur| (&Focus::Tweet, cur));
        let any = self.focus_map.iter().next();

        if let Some((focus, cursor)) = desired.or(default).or(any) {
            self.focus = focus.clone();
            self.scroll_buffer.move_cursor_to(cursor.0, cursor.1);
        } else {
            self.focus = Focus::Tweet;
            self.scroll_buffer.move_cursor_to(0, 0);
        }
    }

    fn update_focus(&mut self, delta: isize) {
        let mut focus_order: Vec<Focus> = Vec::new();

        {
            let tweet_details = self.tweet_details.lock().unwrap();
            let TweetDetails {
                in_reply_to_ids,
                quote_id,
                reply_ids,
                ..
            } = &*tweet_details;

            let num_in_reply_to_ids = in_reply_to_ids.as_ref().map(|v| v.len()).unwrap_or(0);
            let num_reply_ids = reply_ids.as_ref().map(|v| v.len()).unwrap_or(0);
            let has_quote = quote_id.is_some();

            for i in 0..num_in_reply_to_ids {
                focus_order.push(Focus::InReplyTo(i));
            }
            focus_order.push(Focus::Tweet);
            if has_quote {
                focus_order.push(Focus::Quote);
            }
            for i in 0..num_reply_ids {
                focus_order.push(Focus::Reply(i));
            }
        }

        let focus_index = focus_order
            .iter()
            .position(|f| f == &self.focus)
            .unwrap_or(0);
        let new_focus_index =
            (focus_index as isize + delta).rem_euclid(focus_order.len() as isize) as usize;
        self.set_focus(&focus_order[new_focus_index]);
    }

    fn update_scroll_buffer_and_focus_map(&mut self) {
        {
            let tweets = self.store.tweets.lock().unwrap();
            let tweet_details = self.tweet_details.lock().unwrap();

            let TweetDetails {
                in_reply_to_ids,
                tweet_id,
                quote_id,
                reply_ids,
            } = &*tweet_details;

            self.scroll_buffer.clear();
            self.focus_map.clear();

            if let Some(in_reply_to_ids) = in_reply_to_ids {
                for (i, in_reply_to_id) in in_reply_to_ids.iter().enumerate() {
                    self.focus_map
                        .insert(Focus::InReplyTo(i), (0, self.scroll_buffer.height()));

                    if let Some(tweet) = tweets.get(in_reply_to_id) {
                        self.scroll_buffer
                            .append(&mut draw_tweet(self.display_width, tweet));
                    } else {
                        self.scroll_buffer
                            .push(draw_tweet_id(self.display_width, in_reply_to_id));
                    }
                    self.scroll_buffer
                        .push(vec![TextSegment::plain("↖ in reply to")]);
                    self.scroll_buffer.push_newline();
                }
            } else {
                self.scroll_buffer
                    .push(vec![TextSegment::plain("<in_reply_to?>")]);
                self.scroll_buffer.push_newline();
            }

            self.focus_map
                .insert(Focus::Tweet, (0, self.scroll_buffer.height()));

            if let Some(tweet) = tweets.get(tweet_id) {
                self.scroll_buffer
                    .append(&mut draw_tweet(self.display_width, tweet));
            } else {
                self.scroll_buffer
                    .push(draw_tweet_id(self.display_width, tweet_id));
            }
            self.scroll_buffer.push_newline();

            if let Some(reply_ids) = reply_ids {
                for (i, reply_id) in reply_ids.iter().enumerate() {
                    let str_indent = "    ↪ ";

                    self.focus_map.insert(
                        Focus::Reply(i),
                        (str_indent.len(), self.scroll_buffer.height()),
                    );

                    let rem_width = self.display_width.saturating_sub(str_indent.len());
                    let mut line = vec![TextSegment::plain(str_indent)];

                    if let Some(tweet) = tweets.get(reply_id) {
                        line.append(&mut draw_tweet_one_line(rem_width, tweet));
                    } else {
                        line.append(&mut draw_tweet_id(rem_width, reply_id));
                    }

                    self.scroll_buffer.push(line);
                }
            } else {
                self.scroll_buffer
                    .push(vec![TextSegment::plain("<reply_ids?>")]);
                self.scroll_buffer.push_newline();
            }

            // TODO: QT / RT
        }

        let current_focus = self.focus.clone();
        self.set_focus(&current_focus);
        self.should_update_scroll_buffer
            .store(false, Ordering::SeqCst);
    }
}

fn draw_tweet_id(_width: usize, tweet_id: &str) -> Vec<TextSegment> {
    vec![TextSegment::plain(&format!("<tweet id: {tweet_id}>"))]
}

fn draw_tweet(width: usize, tweet: &api::Tweet) -> Vec<Vec<TextSegment>> {
    let mut buffer = Vec::new();
    let str_unknown = String::from("[unknown]");
    let tweet_time = tweet.created_at.format("%Y-%m-%d %H:%M:%S");
    let tweet_author_username = tweet.author_username.as_ref().unwrap_or(&str_unknown);
    let tweet_author_name = tweet.author_name.as_ref().unwrap_or(&str_unknown);
    let tweet_lines = textwrap::wrap(&tweet.text, width.saturating_sub(1) as usize);

    // CR-someday: DSL quote macro, if worthwhile
    buffer.push(vec![TextSegment::plain(&format!("{tweet_time}"))]);
    buffer.push(vec![TextSegment::plain(&format!(
        "@{tweet_author_username} [{tweet_author_name}]"
    ))]);
    buffer.push(vec![]);

    for line in tweet_lines {
        buffer.push(vec![TextSegment::plain(&line)]);
    }

    buffer
}

fn draw_tweet_one_line(width: usize, tweet: &api::Tweet) -> Vec<TextSegment> {
    // CR: factor str_unknown to 'static
    let str_unknown = String::from("[unknown]");
    let tweet_author = tweet.author_username.as_ref().unwrap_or(&str_unknown);
    let tweet_author = format!("@{tweet_author} ");

    let mut line = vec![
        TextSegment::plain("    ↪ "),
        TextSegment::color(&tweet_author, Colors::new(Color::DarkCyan, Color::Black)),
    ];

    // TODO: this should be factored, same as feed_pane
    let re_newlines = Regex::new(r"[\r\n]+").unwrap();
    let formatted = re_newlines.replace_all(&tweet.text, "⏎ ");
    let remaining_length = width.saturating_sub(tweet_author.len() + 6) as usize;
    let lines = textwrap::wrap(&formatted, remaining_length);
    if lines.len() == 1 {
        line.push(TextSegment::plain(&lines[0]));
    } else if lines.len() > 1 {
        // Rewrap lines to accommodate ellipsis (…), which may knock out a word
        let remaining_length = remaining_length.saturating_sub(1);
        let lines = textwrap::wrap(&formatted, remaining_length);
        line.push(TextSegment::plain(&format!("{}…", &lines[0])));
    }

    line
}

// CR-soon: probably factor out some of this, but need to think of the right abstraction
impl Render for TweetPane {
    fn should_render(&self) -> bool {
        self.should_update_scroll_buffer.load(Ordering::SeqCst)
            || self.scroll_buffer.should_render()
    }

    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        let BoundingBox {
            left,
            top,
            width,
            height,
        } = bounding_box;

        if self.should_update_scroll_buffer.load(Ordering::SeqCst)
            || self.display_width != width as usize
        {
            self.display_width = width as usize;
            self.update_scroll_buffer_and_focus_map();
        }

        if self.scroll_buffer.should_render() {
            let str_clear = " ".repeat(width as usize);
            for y_offset in 0..height {
                queue!(stdout, cursor::MoveTo(left, top + y_offset))?;
                queue!(stdout, style::Print(&str_clear))?;
            }

            self.scroll_buffer.render(stdout, bounding_box)?;
        }

        stdout.flush()?;
        Ok(())
    }

    fn get_cursor(&self) -> (u16, u16) {
        self.scroll_buffer.get_cursor()
    }
}

impl Input for TweetPane {
    fn handle_focus(&mut self) {
        self.scroll_buffer.handle_focus()
    }

    fn handle_key_event(&mut self, event: &KeyEvent) -> bool {
        match event.code {
            KeyCode::Up => (),
            KeyCode::Down => (),
            _ => return self.scroll_buffer.handle_key_event(event),
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segmentation() {
        // NB: expectation is correct; reasoning may be subtle
        let str =
            "Why did the chicken cross the road?\n\nBecause he wanted to get to the other side.";
        let result = textwrap::wrap(str, 20);
        assert_eq!(
            result,
            vec![
                "Why did the chicken",
                "cross the road?",
                "",
                "Because he wanted",
                "to get to the other",
                "side."
            ]
        );
    }

    #[test]
    fn test_focus_eq() {
        let l = Focus::InReplyTo(3);
        let r = Focus::InReplyTo(3);
        assert_eq!(l, r);
    }
}
