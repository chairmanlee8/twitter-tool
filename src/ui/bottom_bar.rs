use crate::ui::{BoundingBox, Input, Render};
use anyhow::Result;
use crossterm::event::KeyEvent;
use crossterm::style::Color;
use crossterm::{cursor, queue, style};
use std::io::{Stdout, Write};
use std::sync::{Arc, Mutex};

pub struct BottomBar {
    tweets_reverse_chronological: Arc<Mutex<Vec<String>>>,
}

impl BottomBar {
    pub fn new(tweets_reverse_chronological: &Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            tweets_reverse_chronological: tweets_reverse_chronological.clone(),
        }
    }
}

impl Render for BottomBar {
    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().unwrap();
        let feed_length = tweets_reverse_chronological.len();

        queue!(stdout, cursor::MoveTo(bounding_box.left, bounding_box.top))?;
        queue!(stdout, style::SetForegroundColor(Color::Black))?;
        queue!(stdout, style::SetBackgroundColor(Color::White))?;
        queue!(stdout, style::Print(format!("{feed_length} tweets")))?;
        queue!(stdout, style::ResetColor)?;

        stdout.flush()?;
        Ok(())
    }
}

impl Input for BottomBar {
    fn handle_key_event(&mut self, _event: KeyEvent) {
        todo!()
    }

    fn get_cursor(&self, _bounding_box: BoundingBox) -> (u16, u16) {
        todo!()
    }
}
