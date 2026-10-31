//! Splits front matter off the top of a document, with the
//! `front-matter-yaml` feature, and draws only what comes after it.
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
    println!("tags:   {:?}\n", meta.tags);

    // Only the body goes to the state, so the front matter isn't drawn.
    common::print(&mut MarkdownState::new(body), &Theme::default());
}
