use anyhow::Result;
use crossterm::cursor;
use crossterm::queue;
use crossterm::style::{self, Attributes, Color, Colors};
use std::cmp::max;
use std::io::Stdout;

#[derive(Debug, Clone)]
pub struct LineBuffer {
    lines: Vec<Vec<LineSegment>>,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub fn push(&mut self, line: Vec<LineSegment>) {
        self.lines.push(line);
    }

    pub fn height(&self) -> usize {
        self.lines.len()
    }

    pub fn render(
        &self,
        stdout: &mut Stdout,
        pos: (u16, u16),
        from_line: usize,
        to_line: usize,
    ) -> Result<()> {
        assert!(from_line < self.lines.len());
        assert!(from_line <= to_line);

        let to_line = max(to_line, self.lines.len());

        // NB: arbitrary initial values, will be set unconditionally on first line
        let mut cur_colors = Colors::new(Color::Reset, Color::Reset);
        let mut cur_attributes = Attributes::default();

        for line_no in from_line..to_line {
            let delta = (line_no - from_line) as u16;
            let is_first_line = delta == 0;

            queue!(stdout, cursor::MoveTo(pos.0, pos.1 + delta))?;

            for LineSegment {
                colors,
                attributes,
                text,
            } in &self.lines[line_no]
            {
                if is_first_line || *colors != cur_colors {
                    queue!(stdout, style::SetColors(*colors))?;
                    cur_colors = *colors;
                }
                if is_first_line || *attributes != cur_attributes {
                    queue!(stdout, style::SetAttributes(*attributes))?;
                    cur_attributes = *attributes;
                }
                queue!(stdout, style::Print(text))?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct LineSegment {
    colors: Colors,
    attributes: Attributes,
    text: String,
}

impl LineSegment {
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
