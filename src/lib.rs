//! Render Markdown into [Ratatui] text.
//!
//! [`MarkdownView`] is a widget that shows a document, scrolled, with links
//! that can be focused from the keyboard or found under a click. Its
//! [`MarkdownState`] holds the document and lays it out again whenever the
//! width changes.
//!
//! Underneath, [`Rendered::render`] lays a document out for a given width
//! and returns its lines along with where each link landed, for drawing it
//! some other way.
//!
//! ```
//! let rendered = ratmark::Rendered::render("# Hi\n\nSee [the docs](https://docs.rs).", 40, &ratmark::Theme::default());
//! assert_eq!(rendered.links[0].url, "https://docs.rs");
//! ```
//!
//! CommonMark plus tables, footnotes, strikethrough, task lists and math are
//! supported. With the `html` feature, HTML embedded in the Markdown is read
//! for its text, links and simple formatting, and without it HTML is left
//! out. Code blocks are syntax highlighted with the `syntax`
//! feature, which is off by default. Code too wide for the page is cut off
//! rather than wrapped, and [`MarkdownState::shift_by`] and
//! [`MarkdownState::shift_at`] shift a block sideways.
//!
//! The `front-matter-yaml` and `front-matter-toml` features, off by default,
//! split YAML or TOML front matter off the top of a document instead of
//! rendering it, and deserialize it with serde. See [`front_matter`].
//!
//! The `kramdown` feature, also off by default, understands a few habits of
//! Markdown written for kramdown, Jekyll's default: attribute lists like
//! `{:.no_toc}` are dropped, `{:toc}` lists become tables of contents, and
//! Markdown inside `markdown="1"` elements is rendered as Markdown. See
//! [`kramdown`].
//!
//! The `native-links` feature, off by default, writes each link's URL into
//! the cells [`MarkdownView`] draws, so backends that support it show links
//! the terminal or browser knows about. See [`MarkdownState::with_base`].
//!
//! [Ratatui]: https://ratatui.rs

use std::ops::Range;

use ratatui_core::style::Style;
use ratatui_core::text::{Line, Span, Text};
use unicode_width::UnicodeWidthChar;

#[cfg(any(feature = "front-matter-yaml", feature = "front-matter-toml"))]
pub mod front_matter;
#[cfg(feature = "kramdown")]
pub mod kramdown;
mod render;
#[cfg(feature = "syntax")]
mod syntax;
mod table;
mod theme;
mod view;
mod wrap;
mod writer;

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
    /// With the `front-matter-yaml` or `front-matter-toml` feature, front
    /// matter at the top is left out. Get it with
    /// [`front_matter::FrontMatter::split`] or
    /// [`MarkdownState::front_matter`].
    pub fn render(markdown: &str, width: u16, theme: &Theme) -> Self {
        #[cfg(any(feature = "front-matter-yaml", feature = "front-matter-toml"))]
        let markdown = front_matter::FrontMatter::split(markdown).1;
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
    pub text: String,
    pub line: usize,
}

/// A link and the places it was drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    /// Where the link's text landed. A link that wraps has one region per
    /// line.
    pub regions: Vec<Region>,
}

/// A code block too wide for the page. Its lines are cut off at the page's
/// edge, with a `›` in the padding where there's more, and can be drawn
/// shifted sideways to show the rest.
#[derive(Debug, Clone, PartialEq)]
pub struct Overflow {
    /// Which of the document's code blocks this is, counting from 0, the
    /// same at any width.
    pub block: usize,
    /// The lines the block is drawn on.
    pub lines: Range<usize>,
    /// The column the block starts at, including its padding.
    pub x: u16,
    /// How many columns of code show, between a column of padding on
    /// either side.
    pub width: u16,
    /// The code, line by line, uncut.
    code: Vec<Line<'static>>,
    style: Style,
}

impl Overflow {
    /// How far the block can be shifted before its widest line runs out.
    pub fn max_shift(&self) -> usize {
        let widest = self.code.iter().map(Line::width).max().unwrap_or(0);
        widest.saturating_sub(usize::from(self.width))
    }

    /// The block's `i`th line with its padding, shifted `shift` columns to
    /// the left, `‹` and `›` showing where code is out of sight.
    pub fn row(&self, i: usize, shift: usize) -> Line<'static> {
        let shift = shift.min(self.max_shift());
        let width = usize::from(self.width);
        let code = &self.code[i];
        let marker =
            |more: bool, c: &'static str| Span::styled(if more { c } else { " " }, self.style);
        let mut spans = vec![marker(shift > 0 && code.width() > 0, "‹")];
        let mut x = 0;
        for span in &code.spans {
            let mut text = String::new();
            for c in span.content.chars() {
                let w = c.width().unwrap_or(0);
                let (from, to) = (x, x + w);
                x = to;
                if to <= shift || from >= shift + width {
                    continue;
                }
                // A wide character cut by either edge shows as spaces.
                if from < shift || to > shift + width {
                    let shown = to.min(shift + width) - from.max(shift);
                    text.push_str(&" ".repeat(shown));
                } else {
                    text.push(c);
                }
            }
            if !text.is_empty() {
                spans.push(Span::styled(text, self.style.patch(span.style)));
            }
        }
        let used = x.saturating_sub(shift).min(width);
        spans.push(Span::styled(" ".repeat(width - used), self.style));
        spans.push(marker(x > shift + width, "›"));
        Line::from(spans)
    }
}

/// A stretch of one line, in columns from the start of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub line: usize,
    pub start: u16,
    pub end: u16,
}
