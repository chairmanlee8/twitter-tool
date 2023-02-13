use crate::api;
use crate::ui::Context;
use crossterm::style::Color;
use crossterm::{cursor, queue, style, Result};
use std::io::{stdout, Write};

pub fn render_bottom_bar(
    context: &Context,
    tweets: &Vec<api::Tweet>,
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
