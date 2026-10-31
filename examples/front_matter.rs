//! Splits front matter off the top of a document, with the
//! `front-matter-yaml` feature, rather than rendering it as text.
//!
//!     cargo run --example front_matter --features front-matter-yaml

mod common;

use ratmark::front_matter::FrontMatter;
use ratmark::{MarkdownState, Theme};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Meta {
    title: String,
    date: String,
    tags: Vec<String>,
}

const MARKDOWN: &str = "---
title: 'Notes: week one'
date: 2026-10-31
tags: [rust, tui]
---

# Week one

The front matter above isn't drawn.
";

fn main() {
    let (front, body) = FrontMatter::split(MARKDOWN);
    let front = front.expect("the document has front matter");
    println!("format: {:?}", front.format);
    let meta: Meta = front.deserialize().expect("the front matter is a Meta");
    println!("title:  {:?}", meta.title);
    println!("date:   {:?}", meta.date);
    println!("tags:   {:?}", meta.tags);
    println!("body:   {} bytes\n", body.len());

    // Rendering the whole document leaves the front matter out too, and the
    // state hands it back.
    let mut state = MarkdownState::new(MARKDOWN);
    assert_eq!(state.front_matter(), Some(front));
    common::print(&mut state, &Theme::default());
}
