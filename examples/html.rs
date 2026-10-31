//! Reads the text, links and simple formatting out of HTML in Markdown,
//! with the `html` feature. Without it, HTML is left out.
//!
//!     cargo run --example html --features html

mod common;

use ratatui_core::style::{Color, Style};
use ratmark::{MarkdownState, Theme};

const MARKDOWN: &str = r#"Markdown from real sites mixes in <em>HTML</em>, like
<a href="https://ratatui.rs">links</a>, <code>code</code> and
<small>small print</small>.

<ul>
  <li>HTML lists</li>
  <li>with <strong>strong</strong> text</li>
</ul>

<div class="warning">
  <p>A classed element, styled by the theme.</p>
  <pre><code>cargo run --example html --features html</code></pre>
</div>

<script>console.log("Scripts and styles are left out.")</script>
"#;

fn main() -> std::io::Result<()> {
    // Classes are written like CSS selectors. A code block inside takes the
    // background.
    let theme = Theme {
        classes: vec![(
            ".warning".into(),
            Style::new()
                .fg(Color::Yellow)
                .bg(Color::Rgb(0x3c, 0x38, 0x36)),
        )],
        ..Theme::default()
    };
    common::print(&mut MarkdownState::new(MARKDOWN), &theme)
}
