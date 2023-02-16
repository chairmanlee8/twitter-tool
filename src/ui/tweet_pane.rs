use crate::twitter_client::api;
use crate::ui::Layout;
use crossterm::style::Color;
use crossterm::terminal::{self, ClearType};
use crossterm::{cursor, queue, style, Result};
use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

pub fn render_tweet_pane(layout: &Layout, tweet: &api::Tweet) -> Result<()> {
    let mut stdout = &layout.stdout;

    let inner_width = layout.tweet_pane_width - 1;
    let re_newlines = Regex::new(r"[\r\n]+").unwrap();
    let str_unknown = String::from("[unknown]");

    let tweet_time = tweet.created_at.format("%Y-%m-%d %H:%M:%S");
    let tweet_author_username = tweet.author_username.as_ref().unwrap_or(&str_unknown);
    let tweet_author_name = tweet.author_name.as_ref().unwrap_or(&str_unknown);
    let tweet_author = format!("@{tweet_author_username} [{tweet_author_name}]");

    let mut row = 0;

    // CR: graphemes is one thing but should split on words then greedy instead
    // CR: some graphemes are double width, need to count correctly
    // CR-someday: use Knuth algorithm
    let tweet_paragraphs: Vec<&str> = re_newlines.split(&tweet.text).collect();
    let tweet_lines: Vec<String> = tweet_paragraphs
        .iter()
        .map(|p| break_lines(p, inner_width as usize))
        .flatten()
        .collect();

    queue!(
        stdout,
        cursor::MoveTo(layout.screen_cols - inner_width, row)
    )?;
    queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
    queue!(stdout, style::Print(&tweet_time))?;
    row += 1;

    queue!(
        stdout,
        cursor::MoveTo(layout.screen_cols - inner_width, row)
    )?;
    queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
    queue!(stdout, style::Print(&tweet_author))?;
    row += 2;

    for tweet_line in tweet_lines {
        queue!(
            stdout,
            cursor::MoveTo(layout.screen_cols - inner_width, row)
        )?;
        queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
        queue!(stdout, style::Print(&tweet_line))?;
        row += 1;
    }

    while row < layout.screen_rows {
        queue!(
            stdout,
            cursor::MoveTo(layout.screen_cols - inner_width, row)
        )?;
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
