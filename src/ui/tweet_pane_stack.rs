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

// TODO:
// on focus, attempt to "resolve" a tweet pane
// that is, load all thread tweets until in_reply_to_ids[0].id == tweet.conversation_id
//     and, load quoted tweet
//     and, load one page of replies

// TODO: at this point, let's research a bit into tui-rs and see
// if some of these problems we're acc-ing haven't simply been solved

pub struct TweetPaneStack {
    events: UnboundedSender<InternalEvent>,
    tweets: Arc<Mutex<HashMap<String, api::Tweet>>>,
    // TODO: consider shifting up and making Arc<Mutex>, not a huge fan of relying on set_selected_tweet_id
    // unlike move_selected_index in feed_pane, which is legitimate, this is a non-local datum
    //selected_tweet_id: Option<String>,
    //stack: Option<TweetPane>,
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
            //stack: None,
            cursor_position: (0, 0),
        }
    }

    // TODO: see above TODO about where selected_tweet_id should live. But now this function also resets the stack
    // maybe rename it to something more appropriate like [select_tweet]
    // TODO: deprecate
    // pub fn set_selected_tweet_id(&mut self, tweet_id: Option<String>) {
    //     self.selected_tweet_id = tweet_id;
    // }

    // pub fn open_tweet_pane(&mut self, tweet_primer: &TweetPrimer) {
    //     let mut tweet_pane = TweetPane::new(&self.tweets, &tweet_primer.tweet_id);
    //     tweet_pane.set_tweet_primer(tweet_primer);
    //     self.stack = Some(tweet_pane);
    // }
    //
    // pub fn push_tweet_pane(&mut self, tweet_primer: &TweetPrimer) {
    //     todo!();
    // }
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

        // if let Some(tweet_pane) = &mut self.stack {
        //     tweet_pane.render(stdout, bounding_box)?;
        // }

        Ok(())
    }

    fn get_cursor(&self) -> (u16, u16) {
        return self.cursor_position;
    }
}

impl Input for TweetPaneStack {
    fn handle_focus(&mut self) {
        // if let Some(tweet_pane) = &self.stack {
        //     let tweet_primer = &tweet_pane.tweet_primer;
        //     self.events
        //         .send(InternalEvent::HydrateSelectedTweet(tweet_primer.clone()))
        //         .unwrap()
        // }
    }

    fn handle_key_event(&mut self, _event: &KeyEvent) -> bool {
        todo!()
    }
}
