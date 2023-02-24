use crate::ui_framework::bounding_box::BoundingBox;
use crate::ui_framework::{Input, Render};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use crossterm::queue;
use crossterm::{cursor, style};
use std::io::{Stdout, Write};

#[derive(Debug)]
pub struct SearchBar {
    pub text_input: String,
    pub caret_position: usize,
    pub should_render: bool,
}

impl SearchBar {
    pub fn new() -> Self {
        Self {
            text_input: "".to_string(),
            caret_position: 0,
            should_render: true,
        }
    }

    // CR-soon: we can avoid clone in some situations with a get_and_clear_text() that swaps
    pub fn get_text(&self) -> String {
        self.text_input.clone()
    }

    pub fn clear(&mut self) {
        self.text_input = "".to_string();
        self.caret_position = 0;
        self.should_render = true;
    }

    fn insert_char_at_caret(&mut self, ch: char) {
        self.text_input.insert(self.caret_position, ch);
        self.caret_position += 1;
        self.should_render = true;
    }

    fn delete_char_at_caret(&mut self) {
        if self.caret_position < self.text_input.len() {
            self.text_input.remove(self.caret_position);
            self.should_render = true;
        }
    }

    fn delete_char_before_caret(&mut self) {
        if self.caret_position > 0 {
            self.caret_position -= 1;
            self.delete_char_at_caret();
        }
    }

    fn move_caret(&mut self, delta: isize) {
        let new_position = self.caret_position as isize + delta;
        if new_position >= 0 && new_position <= self.text_input.len() as isize {
            self.caret_position = new_position as usize;
            self.should_render = true;
        }
    }
}

impl Render for SearchBar {
    fn should_render(&self) -> bool {
        self.should_render
    }

    fn invalidate(&mut self) {
        self.should_render = true;
    }

    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        let BoundingBox { left, top, .. } = bounding_box;

        queue!(stdout, cursor::MoveTo(left, top))?;
        queue!(stdout, style::Print("> "))?;

        // CR-soon: search bar horizontal scrolling
        let str_clear = " ".repeat(bounding_box.width.saturating_sub(2) as usize);
        queue!(stdout, style::Print(str_clear))?;
        queue!(stdout, cursor::MoveTo(left + 2, top))?;
        queue!(stdout, style::Print(&self.text_input))?;

        stdout.flush()?;
        Ok(())
    }

    fn get_cursor(&self) -> (u16, u16) {
        (self.caret_position as u16 + 2, 0)
    }
}

impl Input for SearchBar {
    fn handle_focus(&mut self) {
        ()
    }

    fn handle_key_event(&mut self, event: &KeyEvent) -> bool {
        match event.code {
            KeyCode::Char(ch) => self.insert_char_at_caret(ch),
            KeyCode::Left => self.move_caret(-1),
            KeyCode::Right => self.move_caret(1),
            KeyCode::Backspace => self.delete_char_before_caret(),
            KeyCode::Delete => self.delete_char_at_caret(),
            _ => return false,
        }
        true
    }
}
