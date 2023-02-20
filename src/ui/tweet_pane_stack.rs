use crate::twitter_client::{api, TwitterClient};
use crate::ui::InternalEvent;
use crate::ui_framework::bounding_box::BoundingBox;
use crate::ui_framework::scroll_buffer::{ScrollBuffer, TextSegment};
use crate::ui_framework::{Input, Render};
use anyhow::Result;
use crossterm::event::KeyEvent;
use crossterm::style::{Color, Colors};
use crossterm::terminal::{self, ClearType};
use crossterm::{cursor, queue, style};
use regex::Regex;
use std::collections::HashMap;
use std::io::Stdout;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum QuoteType {
    #[default]
    Retweet,
    QuoteTweet,
}

// TODO:
// on focus, attempt to "resolve" a tweet pane
// that is, load all thread tweets until in_reply_to_ids[0].id == tweet.conversation_id
//     and, load quoted tweet
//     and, load one page of replies

// TODO: break ui into ui_framework and ui_components
// TODO: factor out a scroll_view (or just `scroll`)
// how does scrolling work?
// consider a line buffer
//
// ---
// -----
// ---
// ----
//
// the top of what's being drawn is the scroll_offset
// the cursor position is at the cursor_position (y)
// we know the last render height as last_known_height
// goal is to keep the cursor in view through both resize and move
// and to minimize the number and magnitude of changes in scroll_offset
// "idealized scrolling"
//
// if the line buffer changes but our semantic focus hasn't
// maintain the same relative cursor offset
//    (old_cursor_y - old_scroll_offset) =
//    (new_cursor_y - new_scroll_offset)
//    solve for new_scroll_offset
//
// then apply the new height
//
// TODO: at this point, let's research a bit into tui-rs and see
// if some of these problems we're acc-ing haven't simply been solved

// TODO: so there's now two types of focus, TAB focus and ARROW focus...
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Focus {
    InReplyTo(usize),
    #[default]
    Tweet,
    Reply(usize),
    Quote,
}

// TODO: think of a better name for this
#[derive(Debug, Clone, Default)]
pub struct TweetPrimer {
    in_reply_to_ids: Vec<String>, // ordered by ascending timestamp
    tweet_id: String,
    quote_id: Option<(QuoteType, String)>,
    reply_ids: Vec<String>,
}

impl TweetPrimer {
    pub fn new(tweet_id: &String) -> Self {
        Self {
            in_reply_to_ids: Vec::new(),
            tweet_id: tweet_id.clone(),
            quote_id: None,
            reply_ids: Vec::new(),
        }
    }

    fn set_in_reply_to_ids(&mut self, in_reply_to_ids: &Vec<String>) {
        self.in_reply_to_ids = in_reply_to_ids.clone();
    }

    fn set_reply_ids(&mut self, reply_ids: &Vec<String>) {
        self.reply_ids = reply_ids.clone();
    }

    fn set_retweet_id(&mut self, retweet_id: &String) {
        self.quote_id = Some((QuoteType::Retweet, retweet_id.clone()));
    }

    fn set_quote_tweet_id(&mut self, quote_tweet_id: &String) {
        self.quote_id = Some((QuoteType::QuoteTweet, quote_tweet_id.clone()));
    }
}

// TODO: move to tweet_pane.rs
#[derive(Debug, Clone, Default)]
pub struct TweetPane {
    tweets: Arc<Mutex<HashMap<String, api::Tweet>>>,
    tweet_primer: TweetPrimer,
    focus: Focus,
    cursor_position: (u16, u16),
    scroll_offset: u16,
    last_known_height: u16,
}

impl TweetPane {
    pub fn new(tweets: &Arc<Mutex<HashMap<String, api::Tweet>>>, tweet_id: &String) -> Self {
        Self {
            tweets: tweets.clone(),
            tweet_primer: TweetPrimer::new(tweet_id),
            focus: Focus::Tweet,
            cursor_position: (0, 0),
            scroll_offset: 0,
            last_known_height: 0,
        }
    }

    pub fn set_tweet_primer(&mut self, tweet_primer: &TweetPrimer) {
        self.tweet_primer = tweet_primer.clone();
    }
}

fn render_tweet(
    line_buffer: &mut ScrollBuffer,
    width: u16,
    tweet_id: &str,
    tweet: &Option<&api::Tweet>,
) {
    if let Some(tweet) = tweet {
        let str_unknown = String::from("[unknown]");
        let tweet_time = tweet.created_at.format("%Y-%m-%d %H:%M:%S");
        let tweet_author_username = tweet.author_username.as_ref().unwrap_or(&str_unknown);
        let tweet_author_name = tweet.author_name.as_ref().unwrap_or(&str_unknown);
        let tweet_lines = textwrap::wrap(&tweet.text, width.saturating_sub(1) as usize);

        // CR-someday: DSL quote macro, if worthwhile
        line_buffer.push(vec![TextSegment::plain(&format!("{tweet_time}"))]);
        line_buffer.push(vec![TextSegment::plain(&format!(
            "@{tweet_author_username} [{tweet_author_name}]"
        ))]);
        line_buffer.push(vec![]);

        for line in tweet_lines {
            line_buffer.push(vec![TextSegment::plain(&line)]);
        }
    } else {
        line_buffer.push(vec![TextSegment::plain(&format!("<tweet id: {tweet_id}>"))]);
    }
}

fn render_tweet_reply(
    line_buffer: &mut ScrollBuffer,
    width: u16,
    reply_id: &str,
    reply: &Option<&api::Tweet>,
) {
    if let Some(reply) = reply {
        // CR: factor str_unknown to 'static
        let str_unknown = String::from("[unknown]");
        let reply_author = reply.author_username.as_ref().unwrap_or(&str_unknown);
        let reply_author = format!("@{reply_author} ");

        let mut line = vec![
            TextSegment::plain("    ↪ "),
            TextSegment::color(&reply_author, Colors::new(Color::DarkCyan, Color::Black)),
        ];

        // TODO: this should be factored, same as feed_pane
        let re_newlines = Regex::new(r"[\r\n]+").unwrap();
        let formatted = re_newlines.replace_all(&reply.text, "⏎ ");
        let remaining_length = width.saturating_sub((reply_author.len() + 6) as u16) as usize;
        let lines = textwrap::wrap(&formatted, remaining_length);
        if lines.len() == 1 {
            line.push(TextSegment::plain(&lines[0]));
        } else if lines.len() > 1 {
            // Rewrap lines to accommodate ellipsis (…), which may knock out a word
            let remaining_length = remaining_length.saturating_sub(1);
            let lines = textwrap::wrap(&formatted, remaining_length);
            line.push(TextSegment::plain(&format!("{}…", &lines[0])));
        }

        line_buffer.push(line);
    } else {
        line_buffer.push(vec![TextSegment::plain(&format!(
            "    ↪ <reply tweet id: {reply_id}>"
        ))]);
    }
}

impl Render for TweetPane {
    fn should_render(&self) -> bool {
        todo!()
    }

    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        let BoundingBox {
            left,
            top,
            width,
            height,
        } = bounding_box;

        self.last_known_height = height;

        let mut line_buffer = ScrollBuffer::new();
        let mut line_cursor_top: usize = 0;
        let tweets = self.tweets.lock().unwrap();

        for (i, in_reply_to_id) in self.tweet_primer.in_reply_to_ids.iter().enumerate() {
            if self.focus == Focus::InReplyTo(i) {
                line_cursor_top = line_buffer.height();
            }

            let tweet = tweets.get(&*in_reply_to_id);
            render_tweet(&mut line_buffer, width, &in_reply_to_id, &tweet);
            line_buffer.push(vec![TextSegment::plain("↖ in reply to")]);
            line_buffer.push(vec![]);
        }

        if self.focus == Focus::Tweet {
            line_cursor_top = line_buffer.height();
        }

        let tweet_id = &self.tweet_primer.tweet_id;
        let tweet = tweets.get(&self.tweet_primer.tweet_id);
        render_tweet(&mut line_buffer, width, tweet_id, &tweet);

        for (i, reply_id) in self.tweet_primer.reply_ids.iter().enumerate() {
            if self.focus == Focus::Reply(i) {
                line_cursor_top = line_buffer.height();
            }

            // TODO: why &*reply_id is needed instead of &reply_id?
            let reply = tweets.get(&*reply_id);
            render_tweet_reply(&mut line_buffer, width, &reply_id, &reply);
        }

        line_buffer.push(vec![]);

        // TODO: QT / RT

        // TODO: compute correct from_line and to_line, requires some thinking about how scrolling works exactly

        // line_buffer.render(stdout, (left, top), 0, height as usize)?;

        Ok(())
    }

    fn get_cursor(&self) -> (u16, u16) {
        todo!()
    }
}

impl Input for TweetPane {
    fn handle_focus(&mut self) {
        todo!()
    }

    fn handle_key_event(&mut self, event: &KeyEvent) {
        todo!()
    }
}

pub struct TweetPaneStack {
    events: UnboundedSender<InternalEvent>,
    tweets: Arc<Mutex<HashMap<String, api::Tweet>>>,
    // TODO: consider shifting up and making Arc<Mutex>, not a huge fan of relying on set_selected_tweet_id
    // unlike move_selected_index in feed_pane, which is legitimate, this is a non-local datum
    //selected_tweet_id: Option<String>,
    stack: Option<TweetPane>,
    // stack: Vec<TweetPane>,
    // focus: Focus,
    // scroll_offset: u16,
    cursor_position: (u16, u16),
}

impl TweetPaneStack {
    pub fn new(
        events: &UnboundedSender<InternalEvent>,
        tweets: &Arc<Mutex<HashMap<String, api::Tweet>>>,
    ) -> Self {
        Self {
            events: events.clone(),
            tweets: tweets.clone(),
            //selected_tweet_id: None,
            stack: None,
            cursor_position: (0, 0),
        }
    }

    // TODO: see above TODO about where selected_tweet_id should live. But now this function also resets the stack
    // maybe rename it to something more appropriate like [select_tweet]
    // TODO: deprecate
    // pub fn set_selected_tweet_id(&mut self, tweet_id: Option<String>) {
    //     self.selected_tweet_id = tweet_id;
    // }

    pub fn open_tweet_pane(&mut self, tweet_primer: &TweetPrimer) {
        let mut tweet_pane = TweetPane::new(&self.tweets, &tweet_primer.tweet_id);
        tweet_pane.set_tweet_primer(tweet_primer);
        self.stack = Some(tweet_pane);
    }

    pub fn push_tweet_pane(&mut self, tweet_primer: &TweetPrimer) {
        todo!();
    }
}

impl Render for TweetPaneStack {
    fn should_render(&self) -> bool {
        todo!()
    }

    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        let BoundingBox {
            left,
            top,
            width,
            height,
        } = bounding_box;

        self.cursor_position = (left, top);

        if let Some(tweet_pane) = &mut self.stack {
            tweet_pane.render(stdout, bounding_box)?;
        }

        Ok(())
    }

    fn get_cursor(&self) -> (u16, u16) {
        return self.cursor_position;
    }
}

impl Input for TweetPaneStack {
    fn handle_focus(&mut self) {
        if let Some(tweet_pane) = &self.stack {
            let tweet_primer = &tweet_pane.tweet_primer;
            self.events
                .send(InternalEvent::HydrateSelectedTweet(tweet_primer.clone()))
                .unwrap()
        }
    }

    fn handle_key_event(&mut self, _event: &KeyEvent) {
        todo!()
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
