//! Writes each link's address into the cells the view draws, with the
//! `links` feature, so backends that support it draw real links. The
//! crossterm backend prints them as OSC 8 links, which most terminals make
//! clickable.
//!
//!     cargo run --example links --features links
//!
//! The feature needs Ratatui's cell extensions, which aren't released yet
//! (ratatui/ratatui#2854), so this crate uses ratatui-core from that branch,
//! and an app using it needs a backend from the same branch with the
//! `cell-extensions` feature, like ratatui-crossterm here.

mod common;

use ratmark::{MarkdownState, Theme};

const MARKDOWN: &str = "Relative links, like [the next page](next) and [the \
home page](/), are made absolute against the document's own address. Others, \
like [Ratatui](https://ratatui.rs), are kept as they are. [A long link that \
wraps onto the next line](https://github.com/ratatui/ratatui/issues/2854) is \
still one link.";

fn main() -> std::io::Result<()> {
    let mut state = MarkdownState::new(MARKDOWN).with_link_base("https://example.com/guide/start");
    common::print(&mut state, &Theme::default())
}
