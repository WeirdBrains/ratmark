//! Walks pulldown-cmark's events and lays the document out in lines.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, HeadingLevel, Options, Tag, TagEnd};
use ratatui_core::style::Style;

use crate::table::Table;
use crate::wrap::{Inline, Piece};
use crate::writer::{Prefix, Writer};
use crate::{Heading, Overflow, Rendered, Theme};

#[cfg(feature = "html")]
mod html;

const BULLETS: [&str; 3] = ["• ", "◦ ", "▪ "];

/// Heading attributes are left off: pulldown-cmark panics reading some of
/// them, and kramdown, the one dialect here with heading ids, has its own.
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_SMART_PUNCTUATION
}

pub(crate) struct Renderer<'t> {
    theme: &'t Theme,
    writer: Writer,
    headings: Vec<Heading>,
    overflows: Vec<Overflow>,
    /// Code blocks written so far.
    code_blocks: usize,
    open: Vec<Open>,
    capture: Option<Capture>,
    table: Option<Table>,
}

impl<'t> Renderer<'t> {
    pub fn new(theme: &'t Theme, width: u16) -> Self {
        Self {
            theme,
            writer: Writer::new(width),
            headings: Vec::new(),
            overflows: Vec::new(),
            code_blocks: 0,
            open: Vec::new(),
            capture: None,
            table: None,
        }
    }

    pub fn run(mut self, markdown: &str) -> Rendered {
        #[cfg(feature = "kramdown")]
        for (event, attributes) in pulldown_kram::Document::new(markdown, options()).events() {
            self.next(event, &attributes.classes);
        }
        #[cfg(not(feature = "kramdown"))]
        for event in pulldown_cmark::Parser::new_ext(markdown, options()) {
            self.next(event, &[]);
        }
        let (lines, links) = self.writer.finish();
        Rendered {
            lines,
            links,
            headings: self.headings,
            overflows: self.overflows,
        }
    }

    /// Handles the next event, which kramdown's attribute lists may give
    /// `classes`.
    #[cfg_attr(not(feature = "kramdown"), allow(unused_variables))]
    fn next(&mut self, event: Event<'_>, classes: &[CowStr<'_>]) {
        match (self.capture.take(), event) {
            (Some(capture), event) => self.captured(capture, event),
            #[cfg(feature = "kramdown")]
            (None, Event::Code(code)) => {
                let style = self.code_style();
                let style = self
                    .theme
                    .class_style(classes.iter().map(|c| c.as_ref()))
                    .map_or(style, |class| style.patch(class));
                self.piece(code.into_string(), style);
            }
            (None, event) => self.event(event),
        }
    }

    /// Handles an event while `capture` is being read.
    fn captured(&mut self, capture: Capture, event: Event<'_>) {
        match (capture, event) {
            (Capture::Code { lang, mut text }, Event::Text(more)) => {
                text.push_str(&more);
                self.capture = Some(Capture::Code { lang, text });
            }
            (Capture::Code { lang, text }, Event::End(TagEnd::CodeBlock)) => {
                self.code_block(&lang, &text)
            }
            (Capture::Image { url, mut alt }, Event::Text(more) | Event::Code(more)) => {
                alt.push_str(&more);
                self.capture = Some(Capture::Image { url, alt });
            }
            (Capture::Image { url, alt }, Event::End(TagEnd::Image)) => self.image(&url, &alt),
            #[cfg(feature = "html")]
            (Capture::HtmlBlock(mut html), Event::Html(more)) => {
                html.push_str(&more);
                self.capture = Some(Capture::HtmlBlock(html));
            }
            #[cfg(feature = "html")]
            (Capture::HtmlBlock(html), Event::End(TagEnd::HtmlBlock)) => self.html_block(&html),
            #[cfg(feature = "html")]
            (pre @ Capture::Pre(_), Event::Html(html) | Event::InlineHtml(html)) => {
                self.capture = Some(pre);
                self.html_tokens(&html);
            }
            #[cfg(feature = "html")]
            (Capture::Pre(mut pre), Event::Text(more)) => {
                pre.push_str(&more);
                self.capture = Some(Capture::Pre(pre));
            }
            (capture, _) => self.capture = Some(capture),
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(text.into_string()),
            Event::Code(code) => {
                let style = self.code_style();
                self.piece(code.into_string(), style);
            }
            Event::InlineMath(math) | Event::DisplayMath(math) => {
                let style = self.style().patch(self.theme.math);
                self.piece(math.into_string(), style);
            }
            #[cfg(feature = "html")]
            Event::Html(html) | Event::InlineHtml(html) => self.html_tokens(&html),
            #[cfg(not(feature = "html"))]
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::FootnoteReference(label) => {
                let style = self.style().patch(self.theme.footnote);
                self.piece(format!("[{label}]"), style);
            }
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.writer.line_break(),
            Event::Rule => {
                self.writer.flush();
                let rule = "─".repeat(self.writer.available());
                self.writer.emit(vec![Piece::new(rule, self.theme.rule)]);
                self.writer.set_gap(true);
            }
            Event::TaskListMarker(done) => {
                let mark = if done { "[x] " } else { "[ ] " };
                self.piece(mark, self.theme.list_marker);
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.writer.flush(),
            Tag::Heading { level, .. } => {
                self.writer.flush();
                let level = heading_index(level);
                self.heading_marker(level);
                self.open(Kind::Heading, self.theme.headings[level]);
            }
            Tag::BlockQuote(_) => {
                self.writer.flush();
                // The gap before a quote goes outside it, without its bar.
                self.writer.pay_gap();
                let bar = Piece::new("▎ ", self.theme.quote_marker);
                self.open_prefixed(Kind::Quote, Prefix::same(bar), self.theme.quote);
            }
            Tag::CodeBlock(kind) => {
                self.writer.flush();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split([' ', ',', '{']).next().unwrap_or("").to_owned()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                self.capture = Some(Capture::Code {
                    lang,
                    text: String::new(),
                });
            }
            Tag::HtmlBlock => {
                self.writer.flush();
                #[cfg(feature = "html")]
                {
                    self.capture = Some(Capture::HtmlBlock(String::new()));
                }
            }
            Tag::List(start) => {
                self.writer.flush();
                self.open(Kind::List(start), Style::new());
            }
            Tag::Item => {
                self.writer.flush();
                let depth = self
                    .open
                    .iter()
                    .filter(|o| matches!(o.kind, Kind::List(_)))
                    .count()
                    .saturating_sub(1);
                let list = self
                    .open
                    .iter_mut()
                    .rev()
                    .find(|o| matches!(o.kind, Kind::List(_)));
                let marker = match list.map(|o| &mut o.kind) {
                    Some(Kind::List(Some(n))) => {
                        *n += 1;
                        format!("{}. ", *n - 1)
                    }
                    _ => BULLETS[depth % BULLETS.len()].to_owned(),
                };
                let marker = Piece::new(marker, self.theme.list_marker);
                let prefix = Prefix::hanging(marker, self.theme.text);
                self.open_prefixed(Kind::Item, prefix, Style::new());
            }
            Tag::FootnoteDefinition(label) => {
                self.writer.flush();
                let marker = Piece::new(format!("[{label}] "), self.theme.footnote);
                let prefix = Prefix::hanging(marker, self.theme.text);
                self.open_prefixed(Kind::Footnote, prefix, Style::new());
            }
            Tag::DefinitionList | Tag::DefinitionListTitle => self.writer.flush(),
            Tag::DefinitionListDefinition => {
                self.writer.flush();
                let indent = Prefix::same(Piece::new("  ", self.theme.text));
                self.open_prefixed(Kind::Definition, indent, Style::new());
            }
            Tag::Table(alignments) => {
                self.writer.flush();
                self.table = Some(Table::new(alignments));
            }
            Tag::TableHead => {
                self.open(Kind::TableHead, self.theme.table_header);
                self.start_row();
            }
            Tag::TableRow => self.start_row(),
            Tag::TableCell => {}
            Tag::Emphasis => self.open(Kind::Style, self.theme.emphasis),
            Tag::Strong => self.open(Kind::Style, self.theme.strong),
            Tag::Strikethrough => self.open(Kind::Style, self.theme.strikethrough),
            Tag::Superscript | Tag::Subscript => self.open(Kind::Style, Style::new()),
            Tag::Link { dest_url, .. } => self.open_link(Kind::Link, &dest_url),
            Tag::Image { dest_url, .. } => {
                self.capture = Some(Capture::Image {
                    url: dest_url.into_string(),
                    alt: String::new(),
                })
            }
            // Never parsed, as `options` leaves metadata blocks off.
            Tag::MetadataBlock(_) => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::DefinitionList => self.writer.end_block(),
            TagEnd::Heading(level) => {
                self.close(|k| *k == Kind::Heading);
                // Skip the `#` marks, if there are any.
                let marks = usize::from(self.theme.heading_marker.is_some());
                let text: String = self
                    .writer
                    .inline()
                    .iter()
                    .skip(marks)
                    .filter_map(|i| match i {
                        Inline::Piece(p) => Some(p.text.as_str()),
                        Inline::Break => None,
                    })
                    .collect();
                self.headings.push(Heading {
                    level: heading_index(level) as u8 + 1,
                    text: text.trim().to_owned(),
                    line: self.writer.next_block_line(),
                });
                self.writer.end_block();
            }
            TagEnd::BlockQuote(_) => {
                self.writer.flush();
                self.close(|k| *k == Kind::Quote);
                self.writer.set_gap(true);
            }
            TagEnd::List(_) => {
                self.writer.flush();
                self.close(|k| matches!(k, Kind::List(_)));
                // A list nested in a tight list's item runs straight on to
                // the next item.
                let nested = self.is_open(|k| matches!(k, Kind::List(_)));
                self.writer.set_gap(!nested);
            }
            TagEnd::Item => {
                self.writer.flush();
                self.close(|k| *k == Kind::Item);
            }
            TagEnd::DefinitionListDefinition => {
                self.writer.flush();
                self.close(|k| *k == Kind::Definition);
            }
            TagEnd::FootnoteDefinition => {
                self.writer.flush();
                self.close(|k| *k == Kind::Footnote);
                self.writer.set_gap(true);
            }
            TagEnd::DefinitionListTitle => self.writer.flush(),
            TagEnd::TableCell => {
                let cell = self.writer.take_inline();
                if let Some(table) = &mut self.table {
                    table.push_cell(cell);
                }
            }
            TagEnd::TableHead => {
                self.close(|k| *k == Kind::TableHead);
            }
            TagEnd::TableRow => {}
            TagEnd::Table => self.table(),
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript => {
                self.close(|k| *k == Kind::Style);
            }
            TagEnd::Link => {
                self.close(|k| *k == Kind::Link);
            }
            // Read whole while captured, in `captured`.
            TagEnd::CodeBlock | TagEnd::Image | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
        }
    }

    // Without the `syntax` feature, the language goes unused.
    #[cfg_attr(not(feature = "syntax"), allow(unused_variables))]
    fn code_block(&mut self, lang: &str, code: &str) {
        let code = code.replace('\t', "    ");
        let base = self.code_block_style();
        let plain = || code.lines().map(|l| vec![Piece::new(l, base)]).collect();
        #[cfg(feature = "syntax")]
        let code = crate::syntax::highlight(&code, lang, self.theme, base).unwrap_or_else(plain);
        #[cfg(not(feature = "syntax"))]
        let code: Vec<Vec<Piece>> = plain();
        self.writer.pay_gap();
        let padded = self.theme.code_block_padding;
        // A column of padding on each side, when the theme wants it.
        let padding = if padded { 2 } else { 0 };
        let first = self.writer.next_block_line();
        let block = Overflow {
            block: self.code_blocks,
            lines: first..first + code.len(),
            x: self.writer.indent() as u16,
            width: self.writer.available().saturating_sub(padding).max(1) as u16,
            code,
            style: base,
            padded,
        };
        for i in 0..block.code.len() {
            self.writer.emit(block.pieces(i, 0));
        }
        self.code_blocks += 1;
        if block.max_shift() > 0 {
            self.overflows.push(block);
        }
        self.writer.set_gap(true);
    }

    fn image(&mut self, url: &str, alt: &str) {
        let label = if alt.trim().is_empty() {
            "image".to_owned()
        } else {
            format!("image: {}", alt.trim())
        };
        let link = self.writer.link(url);
        let style = self.style().patch(self.theme.link).patch(self.theme.image);
        self.writer.piece(format!("[{label}]"), style, Some(link));
    }

    fn table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        for line in table.lines(self.writer.available(), self.theme) {
            self.writer.emit(line);
        }
        self.writer.set_gap(true);
    }

    fn start_row(&mut self) {
        if let Some(table) = &mut self.table {
            table.start_row();
        }
    }

    /// The theme's `#` marks for a heading at `level`, counted from 0.
    fn heading_marker(&mut self, level: usize) {
        if let Some(style) = self.theme.heading_marker {
            self.piece(format!("{} ", "#".repeat(level + 1)), style);
        }
    }

    fn style(&self) -> Style {
        self.open.last().map_or(self.theme.text, |o| o.style)
    }

    fn code_style(&self) -> Style {
        self.style().patch(self.theme.code)
    }

    /// The style of a code block here, with the background of the classes
    /// it's inside, when they have one.
    fn code_block_style(&self) -> Style {
        #[cfg(feature = "html")]
        if let Some(background) = self.open.iter().rev().find_map(|o| match o.kind {
            Kind::Class { background, .. } => background,
            _ => None,
        }) {
            return self.theme.code_block.bg(background);
        }
        self.theme.code_block
    }

    fn link(&self) -> Option<usize> {
        self.open.iter().rev().find_map(|o| o.link)
    }

    fn piece(&mut self, text: impl Into<String>, style: Style) {
        let link = self.link();
        self.writer.piece(text, style, link);
    }

    fn text(&mut self, text: impl Into<String>) {
        let style = self.style();
        self.piece(text, style);
    }

    /// Opens an element whose text is styled with `patch`.
    fn open(&mut self, kind: Kind, patch: Style) {
        let style = self.style().patch(patch);
        self.open.push(Open {
            kind,
            style,
            link: None,
            prefixed: false,
        });
    }

    /// Opens an element that draws `prefix` before each of its lines.
    fn open_prefixed(&mut self, kind: Kind, prefix: Prefix, patch: Style) {
        self.writer.push_prefix(prefix);
        self.open(kind, patch);
        if let Some(open) = self.open.last_mut() {
            open.prefixed = true;
        }
    }

    fn open_link(&mut self, kind: Kind, url: &str) {
        let link = self.writer.link(url);
        self.open(kind, self.theme.link);
        if let Some(open) = self.open.last_mut() {
            open.link = Some(link);
        }
    }

    fn is_open(&self, kind: impl Fn(&Kind) -> bool) -> bool {
        self.open.iter().any(|o| kind(&o.kind))
    }

    /// Closes the innermost open element of `kind`, and anything left open
    /// inside it, like unclosed HTML.
    fn close(&mut self, kind: impl Fn(&Kind) -> bool) {
        if !self.is_open(&kind) {
            return;
        }
        while let Some(open) = self.open.pop() {
            if open.prefixed {
                self.writer.pop_prefix();
            }
            if kind(&open.kind) {
                return;
            }
        }
    }
}

/// An element that's open, and what it changed until it closes.
struct Open {
    kind: Kind,
    /// The style of text inside it.
    style: Style,
    /// The link text inside it points at.
    link: Option<usize>,
    /// Whether it pushed a prefix, to pop when it closes.
    prefixed: bool,
}

/// What an open element is, so its end closes the right one.
#[derive(PartialEq)]
enum Kind {
    /// Inline formatting, like emphasis.
    Style,
    Link,
    Heading,
    Quote,
    /// A list, counting its items when it's numbered.
    List(Option<u64>),
    Item,
    Footnote,
    Definition,
    TableHead,
    /// An HTML element that styles its text, or a link, by its tag name.
    #[cfg(feature = "html")]
    Html(String),
    #[cfg(feature = "html")]
    HtmlItem,
    /// An HTML element with classes the theme styles, by its tag name, and
    /// the background its classes give it.
    #[cfg(feature = "html")]
    Class {
        name: String,
        background: Option<ratatui_core::style::Color>,
    },
}

/// Content read whole before it's drawn, swallowing the events inside it.
enum Capture {
    Code {
        lang: String,
        text: String,
    },
    Image {
        url: String,
        alt: String,
    },
    #[cfg(feature = "html")]
    HtmlBlock(String),
    /// The text of an HTML `<pre>`, kept as is until it closes.
    #[cfg(feature = "html")]
    Pre(String),
}

fn heading_index(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 0,
        HeadingLevel::H2 => 1,
        HeadingLevel::H3 => 2,
        HeadingLevel::H4 => 3,
        HeadingLevel::H5 => 4,
        HeadingLevel::H6 => 5,
    }
}
