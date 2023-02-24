use crate::ui_framework::bounding_box::BoundingBox;
use crate::ui_framework::{Input, Render};
use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{KeyCode, KeyEvent};
use crossterm::queue;
use crossterm::style::{self, Attributes, Color, Colors};
use std::cmp::{max, min};
use std::io::{Stdout, Write};

#[derive(Debug, Clone)]
pub struct ScrollBuffer {
    lines: Vec<Vec<TextSegment>>,
    display_height: usize,
    display_offset: usize,
    cursor_position: (usize, usize),
    should_render: bool,
    // CR: need to work bounding_box != last_bounding_box => should_render into the framework
    last_bounding_box: BoundingBox,
}

impl ScrollBuffer {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            display_height: 0,
            display_offset: 0,
            cursor_position: (0, 0),
            should_render: true,
            last_bounding_box: BoundingBox::default(),
        }
    }

    pub fn push(&mut self, line: Vec<TextSegment>) {
        self.lines.push(line);
        // CR: not optimal
        self.should_render = true;
    }

    pub fn push_newline(&mut self) {
        self.push(vec![]);
    }

    pub fn append(&mut self, lines: &mut Vec<Vec<TextSegment>>) {
        self.lines.append(lines);
        // CR: not optimal
        self.should_render = true;
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.should_render = true;
    }

    pub fn height(&self) -> usize {
        self.lines.len()
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let line_no = max(0, self.cursor_position.1 as isize + delta) as usize;
        self.move_cursor_to(self.cursor_position.0, line_no);
    }

    // CR-soon: this API has turned a bit wonky
    pub fn move_cursor_to(&mut self, x_offset: usize, line_no: usize) {
        let new_offset = min(line_no, self.lines.len().saturating_sub(1));

        if new_offset < self.display_offset {
            self.display_offset = new_offset;
            self.should_render = true;
        } else if new_offset >= self.display_offset + self.display_height {
            self.display_offset = new_offset - self.display_height + 1;
            self.should_render = true;
        }

        self.cursor_position = (x_offset, new_offset);
    }

    pub fn get_cursor_line(&self) -> usize {
        self.cursor_position.1
    }
}

impl Render for ScrollBuffer {
    fn should_render(&self) -> bool {
        self.should_render
    }

    fn invalidate(&mut self) {
        self.should_render = true;
    }

    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()> {
        if bounding_box != self.last_bounding_box {
            self.last_bounding_box = bounding_box;
            self.should_render = true;
        }

        if self.should_render {
            let BoundingBox {
                left,
                top,
                width,
                height,
            } = bounding_box;

            if self.display_height != height as usize {
                self.display_height = height as usize;
                self.move_cursor(0); // NB: recalculate scroll
            }

            let str_clear = " ".repeat(width as usize);
            let from_line = min(self.display_offset, self.lines.len());
            let to_line = min(self.display_offset + self.display_height, self.lines.len());

            for line_no in from_line..to_line {
                let delta = (line_no - from_line) as u16;

                queue!(stdout, cursor::MoveTo(left, top + delta))?;
                queue!(stdout, style::ResetColor)?;
                queue!(stdout, style::SetAttributes(Attributes::default()))?;
                queue!(stdout, style::Print(&str_clear))?;
                queue!(stdout, cursor::MoveTo(left, top + delta))?;

                for TextSegment {
                    colors,
                    attributes,
                    text,
                } in &self.lines[line_no]
                {
                    queue!(stdout, style::SetColors(*colors))?;
                    queue!(stdout, style::SetAttributes(*attributes))?;
                    queue!(stdout, style::Print(text))?;
                }
            }

            stdout.flush()?;
            self.should_render = false;
        }

        Ok(())
    }

    fn get_cursor(&self) -> (u16, u16) {
        (
            self.cursor_position.0 as u16,
            self.cursor_position.1.saturating_sub(self.display_offset) as u16,
        )
    }
}

impl Input for ScrollBuffer {
    fn handle_focus(&mut self) {
        ()
    }

    fn handle_key_event(&mut self, event: &KeyEvent) -> bool {
        match event.code {
            KeyCode::Up => self.move_cursor(-1),
            KeyCode::Down => self.move_cursor(1),
            _ => return false,
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct TextSegment {
    colors: Colors,
    attributes: Attributes,
    text: String,
}

impl TextSegment {
    pub fn new(text: &str, colors: Colors, attributes: Attributes) -> Self {
        Self {
            colors,
            attributes,
            text: text.to_string(),
        }
    }

    pub fn color(text: &str, colors: Colors) -> Self {
        Self::new(text, colors, Attributes::default())
    }

    pub fn plain(text: &str) -> Self {
        Self::new(
            text,
            Colors::new(Color::Reset, Color::Reset),
            Attributes::default(),
        )
    }
}
