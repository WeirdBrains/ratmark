//! Walks pulldown-cmark's events and lays the document out in lines.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};
use ratatui_core::style::Style;

use crate::table::Table;
use crate::wrap::{Inline, Piece};
use crate::writer::{Prefix, Writer};
use crate::{Heading, Overflow, Rendered, Theme};

#[cfg(feature = "html")]
mod html;

const BULLETS: [&str; 3] = ["• ", "◦ ", "▪ "];

/// What's read beyond CommonMark, without kramdown. Heading attributes are
/// left off, as pulldown-cmark panics reading some of them.
#[cfg(not(feature = "kramdown"))]
const OPTIONS: pulldown_cmark::Options = pulldown_cmark::Options::ENABLE_TABLES
    .union(pulldown_cmark::Options::ENABLE_FOOTNOTES)
    .union(pulldown_cmark::Options::ENABLE_STRIKETHROUGH)
    .union(pulldown_cmark::Options::ENABLE_TASKLISTS)
    .union(pulldown_cmark::Options::ENABLE_MATH)
    .union(pulldown_cmark::Options::ENABLE_SMART_PUNCTUATION);

pub(crate) struct Renderer<'t> {
    theme: &'t Theme,
    writer: Writer,
    headings: Vec<Heading>,
    overflows: Vec<Overflow>,
    /// Code blocks written so far.
    code_blocks: usize,
    /// Where the open heading's text starts in the inline content, past its
    /// `#` marks.
    heading: Option<usize>,
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
            heading: None,
            open: Vec::new(),
            capture: None,
            table: None,
        }
    }

    pub fn run(mut self, markdown: &str) -> Rendered {
        #[cfg(feature = "kramdown")]
        for (event, attributes) in
            pulldown_kram::Document::new(markdown, pulldown_kram::Extensions::all()).events()
        {
            self.next(event, &attributes.classes);
        }
        #[cfg(not(feature = "kramdown"))]
        for event in pulldown_cmark::Parser::new_ext(markdown, OPTIONS) {
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
    fn next(&mut self, event: Event<'_>, classes: &[CowStr<'_>]) {
        if self.capture.is_some() {
            self.captured(&event);
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(text.into_string()),
            Event::Code(code) => {
                let style = self.code_style(classes);
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

    /// Reads `event` into the content being captured, drawing it once it
    /// ends. Anything else inside it is left out.
    fn captured(&mut self, event: &Event<'_>) {
        match (self.capture.as_mut(), event) {
            (Some(Capture::Code { text, .. }), Event::Text(more)) => text.push_str(more),
            (Some(Capture::Image { alt, .. }), Event::Text(more) | Event::Code(more)) => {
                alt.push_str(more)
            }
            #[cfg(feature = "html")]
            (Some(Capture::HtmlBlock(html)), Event::Html(more)) => html.push_str(more),
            #[cfg(feature = "html")]
            (Some(Capture::Pre(pre)), Event::Text(more)) => pre.push_str(more),
            #[cfg(feature = "html")]
            (Some(Capture::Pre(_)), Event::Html(html) | Event::InlineHtml(html)) => {
                self.html_tokens(html)
            }
            (Some(capture), Event::End(end)) if capture.ends_at(end) => self.draw_capture(),
            _ => {}
        }
    }

    /// Draws the content captured, now that it's read whole.
    fn draw_capture(&mut self) {
        match self.capture.take() {
            Some(Capture::Code { lang, text }) => self.code_block(&lang, &text),
            Some(Capture::Image { url, alt }) => self.image(&url, &alt),
            #[cfg(feature = "html")]
            Some(Capture::HtmlBlock(html)) => self.html_block(&html),
            #[cfg(feature = "html")]
            Some(Capture::Pre(pre)) => self.pre(&pre),
            None => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.writer.flush(),
            Tag::Heading { level, .. } => {
                self.writer.flush();
                let level = level as usize - 1;
                self.heading_marker(level);
                self.heading = Some(self.writer.inline().len());
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
                self.close(&Kind::Heading);
                let from = self.heading.take().unwrap_or(0);
                let text: String = self.writer.inline()[from..]
                    .iter()
                    .filter_map(|i| match i {
                        Inline::Piece(p) => Some(p.text.as_str()),
                        Inline::Break => None,
                    })
                    .collect();
                self.headings.push(Heading {
                    level: level as u8,
                    text: text.trim().to_owned(),
                    line: self.writer.next_block_line(),
                });
                self.writer.end_block();
            }
            TagEnd::BlockQuote(_) => {
                self.writer.flush();
                self.close(&Kind::Quote);
                self.writer.set_gap(true);
            }
            TagEnd::List(_) => {
                self.writer.flush();
                self.close(&Kind::List(None));
                // A list nested in a tight list's item runs straight on to
                // the next item.
                let nested = self.is_open(&Kind::List(None));
                self.writer.set_gap(!nested);
            }
            TagEnd::Item => {
                self.writer.flush();
                self.close(&Kind::Item);
            }
            TagEnd::DefinitionListDefinition => {
                self.writer.flush();
                self.close(&Kind::Definition);
            }
            TagEnd::FootnoteDefinition => {
                self.writer.flush();
                self.close(&Kind::Footnote);
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
                self.close(&Kind::TableHead);
            }
            TagEnd::TableRow => {}
            TagEnd::Table => self.table(),
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript => {
                self.close(&Kind::Style);
            }
            TagEnd::Link => {
                self.close(&Kind::Link);
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

    /// The style of inline code, patched with the theme's style for
    /// `classes`, which only kramdown gives it.
    #[cfg_attr(not(feature = "kramdown"), allow(unused_variables))]
    fn code_style(&self, classes: &[CowStr<'_>]) -> Style {
        let style = self.style().patch(self.theme.code);
        #[cfg(feature = "kramdown")]
        if let Some(class) = self.theme.class_style(classes.iter().map(|c| c.as_ref())) {
            return style.patch(class);
        }
        style
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

    fn is_open(&self, kind: &Kind) -> bool {
        self.open.iter().any(|o| o.kind.is_like(kind))
    }

    /// Closes the innermost open element like `kind`, and anything left
    /// open inside it, like unclosed HTML.
    fn close(&mut self, kind: &Kind) {
        if !self.is_open(kind) {
            return;
        }
        while let Some(open) = self.open.pop() {
            if open.prefixed {
                self.writer.pop_prefix();
            }
            if open.kind.is_like(kind) {
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

impl Kind {
    /// Whether this is the same kind of element as `other`: any list, or
    /// HTML of the same tag name.
    fn is_like(&self, other: &Kind) -> bool {
        match (self, other) {
            #[cfg(feature = "html")]
            (Kind::Html(a), Kind::Html(b)) => a == b,
            #[cfg(feature = "html")]
            (Kind::Class { name: a, .. }, Kind::Class { name: b, .. }) => a == b,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

impl Capture {
    /// Whether `end` ends this content. A `<pre>` ends at its closing tag
    /// instead, read from the HTML.
    fn ends_at(&self, end: &TagEnd) -> bool {
        match self {
            Capture::Code { .. } => *end == TagEnd::CodeBlock,
            Capture::Image { .. } => *end == TagEnd::Image,
            #[cfg(feature = "html")]
            Capture::HtmlBlock(_) => *end == TagEnd::HtmlBlock,
            #[cfg(feature = "html")]
            Capture::Pre(_) => false,
        }
    }
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
