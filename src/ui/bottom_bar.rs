use crate::ui::{BoundingBox, Input, Render};
use anyhow::Result;
use crossterm::event::KeyEvent;
use crossterm::style::Color;
use crossterm::terminal::{self, ClearType};
use crossterm::{cursor, queue, style};
use std::io::{Stdout, Write};
use std::sync::{Arc, Mutex};

pub struct BottomBar {
    tweets_reverse_chronological: Arc<Mutex<Vec<String>>>,
    num_tasks_in_flight: usize,
}

impl BottomBar {
    pub fn new(tweets_reverse_chronological: &Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            tweets_reverse_chronological: tweets_reverse_chronological.clone(),
            num_tasks_in_flight: 0,
        }
    }

    pub fn set_num_tasks_in_flight(&mut self, num_tasks_in_flight: usize) {
        self.num_tasks_in_flight = num_tasks_in_flight;
    }
}

impl Render for BottomBar {
    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        let tweets_reverse_chronological = self.tweets_reverse_chronological.lock().unwrap();
        let feed_length = tweets_reverse_chronological.len();

        queue!(stdout, cursor::MoveTo(bounding_box.left, bounding_box.top))?;
        queue!(stdout, style::SetForegroundColor(Color::Black))?;
        queue!(stdout, style::SetBackgroundColor(Color::White))?;

        if self.num_tasks_in_flight > 0 {
            queue!(
                stdout,
                style::Print(format!("[* {}] ", self.num_tasks_in_flight))
            )?;
        }
        queue!(stdout, style::Print(format!("{feed_length} tweets")))?;
        queue!(stdout, style::ResetColor)?;
        queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;

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
