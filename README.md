# Scratchpad

Every component should handle it's own render invalidation.
Implementers should be careful not to "always re-render".
Cursor always rechecked and moved if UI detects it moved.

# TODO

- figure out how to highlight only accounts w/ notifs on (or seed such a list)
- show threads as [n]
- consider moving to GUI already? or maybe see how far unicode gfx can go
- page up/page down tweets (alt, left-[ right-])
- index tweets? for vim style jump
- priority accounts (highlight, show condensed listing)
- adjust timestamps to local tz
- expand images/replies/QTs/related tweets
- show bars based on public_metrics
- live streaming updates
- lens on specific user's tweets
- lens on specific hash tag
- lens on search term
- maybe solve double-buffering to fix flashing on full re-render
- side pane with full tweet expansion (incl. multiline)
- for displaying tweets, prefix char (highlight for select, _^=reply, RT=retweet, QT=quote tweet)
- space to expand reply, RT, QT inline
- consider storing tweet ids as u64; at the very least, type alias the String
- remove "as usize" casts these are unsafe?
- test resize behavior

# Some Thoughts

- A twitter client that can have features built on top
- Features such as, author tagging (beyond abusing notifs), ticker+marketdata detection, rich annotation, etc.
- Somewhat opposed to, e.g. an extension that has features grafted on top
- A twitter client that is more performant and concise
- All tty/cli tools based on a "less is more" philosophy
- GUI tools for some reason require way more investment to get the same performance; ongoing maintenance higher as well
- Is that true? Perhaps I simply haven't yet encountered a lot of complexity that the TUI _will_ bring up eventually
- I think GUI _ought to_ dominate TUI in all aspects, even abstraction cost
- EXCEPT for remote streaming, where bandwidth needed for pixels >>> glyphs (even with diff/compression)
- ALSO EXCEPT for TUIs live inside a terminal ecosystem which might be compelling to some users
- Read more about the Go guy's TUI library and philosophy for a counterargument
- In TUI, w/o sixel or something similar, images really suffer, maybe better to show an AI-gen caption instead...
- ...and defer to "open"...honestly better to just lift to tauri

# More in the Future

- Interleave news from other sources, not just Twitter--steps towards a "bloomberg"ish