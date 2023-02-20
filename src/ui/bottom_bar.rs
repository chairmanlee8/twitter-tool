use crate::store::Store;
use crate::ui_framework::{bounding_box::BoundingBox, Input, Render};
use anyhow::Result;
use crossterm::event::KeyEvent;
use crossterm::style::Color;
use crossterm::terminal::{self, ClearType};
use crossterm::{cursor, queue, style};
use std::io::{Stdout, Write};
use std::sync::Arc;

pub struct BottomBar {
    store: Arc<Store>,
    num_tasks_in_flight: usize,
    should_render: bool,
}

impl BottomBar {
    pub fn new(store: &Arc<Store>) -> Self {
        Self {
            store: store.clone(),
            num_tasks_in_flight: 0,
            should_render: true,
        }
    }

    pub fn set_num_tasks_in_flight(&mut self, n: usize) {
        self.num_tasks_in_flight = n;
        self.should_render = true;
    }
}

impl Render for BottomBar {
    fn should_render(&self) -> bool {
        self.should_render
    }

    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        let tweets_reverse_chronological = self.store.tweets_reverse_chronological.lock().unwrap();
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

    fn get_cursor(&self) -> (u16, u16) {
        todo!()
    }
}

impl Input for BottomBar {
    fn handle_focus(&mut self) {
        todo!()
    }

    fn handle_key_event(&mut self, _event: &KeyEvent) {
        todo!()
    }
}
