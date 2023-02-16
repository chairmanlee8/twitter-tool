use crate::twitter_client::api;
use crate::ui::Layout;
use anyhow::Result;
use crossterm::style::{self, Color};
use crossterm::{cursor, queue};
use regex::Regex;
use std::collections::HashMap;
use unicode_truncate::UnicodeTruncateStr;

pub struct FeedPane;

impl FeedPane {
    pub fn render(
        &self,
        layout: &Layout,
        tweets: &HashMap<String, api::Tweet>,
        tweets_reverse_chronological: &Vec<String>,
        view_offset: usize,
    ) -> Result<()> {
        let mut stdout = &layout.stdout;

        let inner_width = layout.feed_pane_width - 2;
        let re_newlines = Regex::new(r"[\r\n]+").unwrap();
        let str_unknown = String::from("[unknown]");

        for i in 0..(layout.screen_rows - 2) {
            if i > tweets_reverse_chronological.len() as u16 {
                break;
            }

            let tweet_id = &tweets_reverse_chronological[view_offset + (i as usize)];
            let tweet = &tweets.get(tweet_id).unwrap();
            let mut col_offset: u16 = 0;

            let tweet_time = tweet.created_at.format("%m-%d %H:%M:%S");
            let tweet_time = format!("{tweet_time}  > ");
            queue!(stdout, cursor::MoveTo(col_offset, i))?;
            queue!(stdout, style::SetForegroundColor(Color::DarkGrey))?;
            queue!(stdout, style::Print(&tweet_time))?;
            queue!(stdout, style::ResetColor)?;
            col_offset += (tweet_time.len() + 1) as u16;

            // CR: possible to cast from String to &str?
            let tweet_author = tweet.author_username.as_ref().unwrap_or(&str_unknown);
            let tweet_author = format!("@{tweet_author}");
            queue!(stdout, cursor::MoveTo(col_offset, i))?;
            queue!(stdout, style::SetForegroundColor(Color::DarkCyan))?;
            queue!(stdout, style::Print(&tweet_author))?;
            queue!(stdout, style::ResetColor)?;
            col_offset += (&tweet_author.len() + 1) as u16;

            let formatted = re_newlines.replace_all(&tweet.text, "⏎ ");
            let (truncated, _) =
                formatted.unicode_truncate((inner_width.saturating_sub(col_offset)) as usize);
            queue!(stdout, cursor::MoveTo(col_offset, i))?;
            queue!(stdout, style::Print(truncated))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex() {
        let re_newlines = Regex::new(r"[\r\n]+").unwrap();
        let str = "Detected new closed trade\n\nTrader: @Burgerinnn\nSymbol: $ETH\nPosition: short ↘\u{fe0f}\nEntry: 1 500.6\nExit: 1 498.2\nProfit: 3 994\nLeverage: 10x\n\nEntry, take profit, stats, leaderboard can be found at https://t.co/EFjrCz4DgD";
        let result = re_newlines.replace_all(str, "⏎ ");
        let expected = "Detected new closed trade⏎ Trader: @Burgerinnn⏎ Symbol: $ETH⏎ Position: short ↘\u{fe0f}⏎ Entry: 1 500.6⏎ Exit: 1 498.2⏎ Profit: 3 994⏎ Leverage: 10x⏎ Entry, take profit, stats, leaderboard can be found at https://t.co/EFjrCz4DgD";
        assert_eq!(result, expected);
    }
}
