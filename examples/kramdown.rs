//! Renders Markdown written for kramdown, Jekyll's default, with the
//! `kramdown` feature.
//!
//!     cargo run --example kramdown --features kramdown

mod common;

use ratatui_core::style::{Color, Style};
use ratmark::kramdown::Kramdown;
use ratmark::{MarkdownState, Theme};

const MARKDOWN: &str = r#"# Release notes
{:.no_toc}

* TOC
{:toc}

## Added

The `--quiet`{:.new} flag.

## Fixed

<div class="note" markdown="1">
Markdown *inside* HTML, thanks to `markdown="1"`.
</div>

## Fixed

A second heading of the same name gets its own id, `fixed-1`.
"#;

fn main() -> std::io::Result<()> {
    // What a CommonMark parser is handed. The table of contents is written
    // out as links to the headings' ids, and the Markdown in the div gets
    // the blank lines it needs.
    println!("{}", Kramdown::to_commonmark(MARKDOWN));

    // Attribute lists are dropped from the text, and a class after inline
    // code styles it.
    let theme = Theme {
        classes: vec![(".new".into(), Style::new().fg(Color::Green))],
        ..Theme::default()
    };
    common::print(&mut MarkdownState::new(MARKDOWN), &theme)
}
