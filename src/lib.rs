//! Render Markdown as a [Ratatui] [`StatefulWidget`], or into Ratatui text.
//!
//! [`MarkdownView`] is a widget that shows a document, scrolled, with links
//! you can select with the keyboard or the mouse. Its [`MarkdownState`]
//! holds the document and lays it out again whenever the width changes.
//!
//! ```
//! use ratmark::{MarkdownState, MarkdownView, Theme};
//! # let backend = ratatui_core::backend::TestBackend::new(40, 5);
//! # let mut terminal = ratatui_core::terminal::Terminal::new(backend)?;
//!
//! let theme = Theme::default();
//! let mut state = MarkdownState::new("# Hi\n\nSee [the docs](https://docs.rs).");
//!
//! terminal.draw(|frame| {
//!     let view = MarkdownView::new(&theme);
//!     frame.render_stateful_widget(view, frame.area(), &mut state);
//! })?;
//!
//! // The link under a click.
//! let clicked = state.link_at(4, 2).map(|l| l.url.as_str());
//! assert_eq!(clicked, Some("https://docs.rs"));
//!
//! // The link focused from the keyboard.
//! state.focus_next();
//! let focused = state.focused().map(|l| l.url.as_str());
//! assert_eq!(focused, Some("https://docs.rs"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Underneath, [`Rendered::render`] lays a document out for a given width
//! and returns its lines along with where each link landed.
//!
//! ```
//! use ratmark::{Rendered, Theme};
//!
//! let theme = Theme::default();
//! let markdown = "# Hi\n\nSee [the docs](https://docs.rs).";
//! let rendered = Rendered::render(markdown, 40, &theme);
//!
//! assert_eq!(rendered.lines[0].to_string(), "Hi");
//! assert_eq!(rendered.lines[0].spans[0].style, theme.headings[0]);
//! assert_eq!(rendered.links[0].url, "https://docs.rs");
//! ```
//!
//! A [`Theme`] holds every style. [`Theme::gruvbox`] is the default, and
//! [`Theme::plain`] uses no colors, so the terminal's own palette shows
//! through. Every style is a public field, so making your own is a struct
//! literal.
//!
//! CommonMark plus tables, footnotes, strikethrough, task lists and math are
//! supported. Code too wide for the page scrolls sideways rather than
//! wrapping.
//!
#![cfg_attr(
    any(
        feature = "links",
        feature = "syntax",
        feature = "html",
        feature = "kramdown",
        feature = "_front-matter",
    ),
    doc = "# Features"
)]
#![cfg_attr(
    feature = "links",
    doc = concat!(
        "- The `links` feature draws real links, which the terminal or browser can open,\n",
        "  on backends that support them. [`MarkdownState::with_link_base`] makes\n",
        "  relative links absolute.",
    )
)]
#![cfg_attr(
    feature = "syntax",
    doc = "- The `syntax` feature highlights code blocks with syntect."
)]
#![cfg_attr(
    feature = "html",
    doc = concat!(
        "- The `html` feature reads HTML embedded in the Markdown for its text, links\n",
        "  and simple formatting.\n",
        "  - `<a href>` is a link.\n",
        "  - `<strong>` and `<b>` are strong.\n",
        "  - `<em>`, `<i>` and `<cite>` are emphasized.\n",
        "  - `<code>`, `<kbd>`, `<samp>` and `<tt>` are code.\n",
        "  - `<s>`, `<del>` and `<strike>` are struck through.\n",
        "  - `<small>`, `<sub>` and `<sup>` are small.\n",
        "  - `<h1>` to `<h6>` are headings.\n",
        "  - `<pre>` is a code block, `<br>` a line break and `<hr>` a rule.\n",
        "  - `<li>` is a bulleted item, in `<ul>` or `<ol>`.\n",
        "  - `<img src alt>` is an image, like Markdown's, and `<svg aria-label>` a\n",
        "    figure.\n",
        "  - `<p>`, `<div>`, `<blockquote>`, `<section>`, `<article>`, `<header>`,\n",
        "    `<footer>`, `<figure>`, `<table>`, `<tr>`, `<dl>`, `<dt>` and `<dd>` start\n",
        "    a new line, and `<td>` and `<th>` are spaced apart.\n",
        "  - Any element's `class` is styled by [`Theme::classes`].\n",
        "  - Other tags show only their text. Comments, and the content of `<script>`,\n",
        "    `<style>`, `<template>`, `<head>` and `<svg>`, are left out.",
    )
)]
#![cfg_attr(
    feature = "kramdown",
    doc = concat!(
        "- The `kramdown` feature reads Markdown written for kramdown, Jekyll's default,\n",
        "  with [pulldown-kram](https://docs.rs/pulldown-kram), which lists how much of\n",
        "  kramdown it understands.\n",
        "  - A list followed by `{:toc}` is a table of contents, linking to each heading\n",
        "    without the class `no_toc`.\n",
        "  - Attribute lists, like `{:.no_toc}`, are taken out of the text, and classes\n",
        "    on inline code, like `` `x`{:.new} ``, are styled by [`Theme::classes`].\n",
        "  - Markdown inside `markdown=\"1\"` elements is rendered as Markdown.\n",
        "  - A line of only a span tag, like `<img>`, starts a paragraph rather than\n",
        "    an HTML block.",
    )
)]
#![cfg_attr(
    feature = "_front-matter",
    doc = concat!(
        "- The `front-matter-yaml` and `front-matter-toml` features add\n",
        "  [`front_matter`], for splitting YAML or TOML front matter off the top of a\n",
        "  document before rendering the rest, and deserializing it with serde.",
    )
)]
//!
//! [Ratatui]: https://ratatui.rs
//! [`StatefulWidget`]: ratatui_core::widgets::StatefulWidget

#![warn(missing_docs)]

use ratatui_core::text::{Line, Text};

mod overflow;
mod render;
mod table;
mod theme;
mod view;
mod wrap;
mod writer;

#[cfg(feature = "_front-matter")]
pub mod front_matter;
#[cfg(feature = "syntax")]
mod syntax;

pub use overflow::Overflow;
pub use theme::Theme;
pub use view::{MarkdownState, MarkdownView};

/// A rendered document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rendered {
    /// The document's lines, already wrapped. Draw them without wrapping.
    pub lines: Vec<Line<'static>>,
    /// Every link, in the order they appear.
    pub links: Vec<Link>,
    /// Every heading, in order, for jumping to a section or listing them.
    pub headings: Vec<Heading>,
    /// Code blocks too wide for the page, which [`Rendered::lines`] shows
    /// cut off.
    pub overflows: Vec<Overflow>,
}

impl Rendered {
    /// Lays `markdown` out in lines at most `width` columns wide.
    ///
    /// Front matter is laid out like any other text.
    #[cfg_attr(
        feature = "_front-matter",
        doc = "Split it off first with [`front_matter::FrontMatter::split`] to leave it out."
    )]
    pub fn render(markdown: &str, width: u16, theme: &Theme) -> Self {
        render::Renderer::new(theme, width).run(markdown)
    }

    /// The lines as a [`Text`].
    pub fn text(&self) -> Text<'static> {
        Text::from(self.lines.clone())
    }

    /// The link drawn at column `x` of line `line`, if there is one.
    pub fn link_at(&self, line: usize, x: u16) -> Option<&Link> {
        self.links.iter().find(|link| {
            link.regions
                .iter()
                .any(|r| r.line == line && (r.start..r.end).contains(&x))
        })
    }
}

/// A heading and the line it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1 for `#` through 6 for `######`.
    pub level: u8,
    /// Its text, without markup.
    pub text: String,
    /// The line it starts on.
    pub line: usize,
}

/// A link and the places it was drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Where it goes, as written in the document.
    pub url: String,
    /// Where the link's text landed. A link that wraps has one region per
    /// line.
    pub regions: Vec<Region>,
}

/// A stretch of one line, in columns from the start of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// The line it's on.
    pub line: usize,
    /// The first column it covers.
    pub start: u16,
    /// The column after the last it covers.
    pub end: u16,
}
