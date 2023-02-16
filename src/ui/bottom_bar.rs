use crate::twitter_client::api;
use crate::ui::Layout;
use anyhow::Result;
use crossterm::style::Color;
use crossterm::{cursor, queue, style};
use std::io::{stdout, Write};

pub struct BottomBar;

impl BottomBar {
    pub fn render(
        &self,
        context: &Layout,
        tweets: &Vec<String>,
        selected_index: usize,
    ) -> Result<()> {
        let mut stdout = stdout();

        queue!(stdout, cursor::MoveTo(0, context.screen_rows - 1))?;
        queue!(stdout, style::SetForegroundColor(Color::Black))?;
        queue!(stdout, style::SetBackgroundColor(Color::White))?;
        queue!(
            stdout,
            style::Print(format!("{}/{} tweets", selected_index, tweets.len()))
        )?;
        queue!(stdout, style::ResetColor)?;

        stdout.flush()?;
        Ok(())
    }
}
