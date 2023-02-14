use crate::api;
use crate::ui::Context;
use crossterm::style::Color;
use crossterm::terminal::{self, ClearType};
use crossterm::{cursor, queue, style, Result};
use regex::Regex;
use std::io::stdout;
use unicode_segmentation::UnicodeSegmentation;
// use itertools::Itertools;

pub fn render_tweet_pane(context: &Context, pane_width: u16, tweet: &api::Tweet) -> Result<()> {
    // CR: move this to Context
    let mut stdout = stdout();

    let re_newlines = Regex::new(r"[\r\n]+").unwrap();
    let str_unknown = String::from("[unknown]");

    // CR: factor these out to impl Tweet
    let tweet_time = tweet.created_at.format("%Y-%m-%d %H:%M:%S");
    let tweet_author = tweet.author_username.as_ref().unwrap_or(&str_unknown);
    let tweet_author = format!("@{tweet_author}");

    let mut row = 0;

    // CR: graphemes is one thing but should split on words then greedy instead
    // CR: some graphemes are double width, need to count correctly
    // CR-soon: use Knuth
    let tweet_paragraphs: Vec<&str> = re_newlines.split(&tweet.text).collect();
    let tweet_lines: Vec<String> = tweet_paragraphs
        .iter()
        .map(|p| break_lines(p, pane_width as usize))
        .flatten()
        .collect();

    queue!(stdout, cursor::MoveTo(context.screen_cols - pane_width - 1, row))?;
    queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
    queue!(stdout, style::Print(&tweet_time))?;
    row += 1;

    queue!(stdout, cursor::MoveTo(context.screen_cols - pane_width - 1, row))?;
    queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
    queue!(stdout, style::Print(&tweet_author))?;
    row += 2;

    for tweet_line in tweet_lines {
        queue!(stdout, cursor::MoveTo(context.screen_cols - pane_width - 1, row))?;
        queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
        queue!(stdout, style::Print(&tweet_line))?;
        row += 1;
    }

    while row < context.screen_rows {
        queue!(stdout, cursor::MoveTo(context.screen_cols - pane_width - 1, row))?;
        queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
        row += 1;
    }

    Ok(())
}

fn break_lines(text: &str, line_width: usize) -> Vec<String> {
    // CR: why does this work?
    UnicodeSegmentation::graphemes(text, true)
        .collect::<Vec<&str>>()
        .chunks(line_width)
        .map(|chunk| chunk.concat())
        .collect::<Vec<String>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segmentation() {
        let str = "Hello world!";
        let lines = break_lines(&str, 4);
        let expected = vec!["Hell", "o wo", "rld!"];
        assert_eq!(lines, expected);
    }
}
