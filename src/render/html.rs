//! The text and tags of the HTML embedded in Markdown, from html5gum's
//! tokenizer. Nothing here builds a tree.

use html5gum::{DefaultEmitter, Token as Html, Tokenizer};
use ratatui_core::style::Style;

use super::{BULLETS, Capture, Kind, Open, Renderer};
use crate::wrap::Piece;
use crate::writer::Prefix;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token {
    Text(String),
    Open {
        name: String,
        attrs: Vec<(String, String)>,
    },
    Close(String),
}

impl Token {
    pub fn attr(&self, key: &str) -> Option<&str> {
        match self {
            Token::Open { attrs, .. } => attrs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }
}

/// Elements that never have content, or a closing tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// Elements whose content is never shown as text.
const SKIPPED: &[&str] = &["script", "style", "svg", "template", "head"];

/// Splits HTML into text and tags. Comments are dropped, along with the
/// content of [`SKIPPED`] elements, though their opening tag is still
/// returned so its attributes can be used.
pub(crate) fn tokenize(html: &str) -> Vec<Token> {
    let mut emitter = DefaultEmitter::default();
    // Read the insides of elements like `<script>` as text, like a browser.
    emitter.naively_switch_states(true);
    let mut tokens: Vec<Token> = Vec::new();
    let mut skipping: Option<String> = None;
    for token in Tokenizer::new_with_emitter(html, emitter).flatten() {
        match token {
            Html::StartTag(tag) if skipping.is_none() => {
                let name = string(&tag.name);
                if SKIPPED.contains(&name.as_str()) && !tag.self_closing {
                    skipping = Some(name.clone());
                }
                let attrs = tag
                    .attributes
                    .iter()
                    .map(|(k, v)| (string(k), string(v)))
                    .collect();
                tokens.push(Token::Open { name, attrs });
            }
            Html::EndTag(tag) => {
                let name = string(&tag.name);
                if skipping.is_none() {
                    tokens.push(Token::Close(name));
                } else if skipping.as_ref() == Some(&name) {
                    skipping = None;
                }
            }
            Html::String(text) if skipping.is_none() => {
                // The tokenizer can hand text over in pieces.
                if let Some(Token::Text(last)) = tokens.last_mut() {
                    last.push_str(&string(&text));
                } else {
                    tokens.push(Token::Text(string(&text)));
                }
            }
            _ => {}
        }
    }
    tokens
}

fn string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl Renderer<'_> {
    /// Draws an HTML block once it's been read whole.
    pub(super) fn html_block(&mut self, html: &str) {
        let before = self.writer.lines_written();
        self.html_tokens(html);
        // A `<pre>` left open ends with the block.
        if matches!(self.capture, Some(Capture::Pre(_))) {
            self.draw_capture();
        }
        // So do HTML styles and links.
        while matches!(
            self.open.last(),
            Some(Open {
                kind: Kind::Html(_),
                ..
            })
        ) {
            self.open.pop();
        }
        // Lots of HTML, like a closing `</div>`, shows nothing, and
        // shouldn't leave a blank line behind.
        if self.writer.inline_is_blank() && self.writer.lines_written() == before {
            self.writer.take_inline();
        } else {
            self.writer.end_block();
        }
    }

    /// Reads the text and links out of a chunk of HTML.
    pub(super) fn html_tokens(&mut self, html: &str) {
        for token in tokenize(html) {
            // Inside a `<pre>`, only its text and its end matter.
            if let Some(Capture::Pre(pre)) = &mut self.capture {
                match &token {
                    Token::Text(text) => pre.push_str(text),
                    Token::Close(name) if name == "pre" => {
                        self.draw_capture();
                        self.close_class(name);
                    }
                    _ => {}
                }
                continue;
            }
            match &token {
                Token::Text(text) => self.text(text.clone()),
                Token::Open { name, .. } => {
                    self.open_class(name, token.attr("class"));
                    self.open_tag(&token, name);
                }
                Token::Close(name) => {
                    self.close_tag(name);
                    self.close_class(name);
                }
            }
        }
    }

    /// Opens a [`Kind::Class`] for an element with classes the theme styles,
    /// or for one with the same name inside a styled element, so the closing
    /// tags pair up with the right one.
    fn open_class(&mut self, name: &str, classes: Option<&str>) {
        if VOID.contains(&name) {
            return;
        }
        let style = classes.and_then(|c| self.theme.class_style(c.split_whitespace()));
        let kind = Kind::Class {
            name: name.to_owned(),
            background: style.and_then(|style| style.bg),
        };
        if style.is_some() || self.is_open(&kind) {
            self.open(kind, style.unwrap_or_default());
        }
    }

    fn close_class(&mut self, name: &str) {
        let kind = Kind::Class {
            name: name.to_owned(),
            background: None,
        };
        self.close(&kind);
    }

    fn open_tag(&mut self, token: &Token, name: &str) {
        match name {
            "a" => {
                if let Some(href) = token.attr("href") {
                    self.open_link(Kind::Html(name.to_owned()), href);
                }
            }
            "br" => self.writer.line_break(),
            "pre" => {
                self.writer.flush();
                self.capture = Some(Capture::Pre(String::new()));
            }
            "li" => {
                // Like a Markdown item, wrapped lines hang under
                // the text, not the bullet.
                self.writer.flush();
                let bullet = Piece::new(BULLETS[0], self.theme.list_marker);
                let prefix = Prefix::hanging(bullet, self.theme.text);
                self.open_prefixed(Kind::HtmlItem, prefix, Style::new());
            }
            "p" | "div" | "ul" | "ol" | "tr" | "table" | "blockquote" | "section" | "article"
            | "header" | "footer" | "figure" | "dl" | "dt" | "dd" => self.writer.hard_break(),
            "hr" => {
                self.writer.hard_break();
                self.text("───");
                self.writer.line_break();
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                // A blank line before a heading, as in Markdown.
                self.writer.hard_break();
                if self.writer.inline().is_empty() {
                    self.writer.set_gap(true);
                } else {
                    self.writer.line_break();
                }
                let level = usize::from(name.as_bytes()[1] - b'1');
                self.heading_marker(level);
                self.open(Kind::Html(name.to_owned()), self.theme.headings[level]);
            }
            "strong" | "b" => self.open(Kind::Html(name.to_owned()), self.theme.strong),
            "em" | "i" | "cite" => self.open(Kind::Html(name.to_owned()), self.theme.emphasis),
            "code" | "kbd" | "tt" | "samp" => {
                self.open(Kind::Html(name.to_owned()), self.theme.code)
            }
            "s" | "del" | "strike" => {
                self.open(Kind::Html(name.to_owned()), self.theme.strikethrough)
            }
            "small" | "sub" | "sup" => self.open(Kind::Html(name.to_owned()), self.theme.small),
            "img" => {
                let src = token.attr("src").unwrap_or("");
                let alt = token.attr("alt").unwrap_or("");
                self.image(src, alt);
            }
            "svg" => {
                if let Some(label) = token.attr("aria-label") {
                    let style = self.style().patch(self.theme.image);
                    self.piece(format!("[figure: {label}]"), style);
                }
            }
            _ => {}
        }
    }

    fn close_tag(&mut self, name: &str) {
        match name {
            "li" if self.is_open(&Kind::HtmlItem) => {
                self.writer.flush();
                self.close(&Kind::HtmlItem);
            }
            "p" | "div" | "li" | "ul" | "ol" | "tr" | "table" | "blockquote" | "section"
            | "article" | "header" | "footer" | "figure" | "dl" | "dt" | "dd" => {
                self.writer.hard_break()
            }
            "td" | "th" => self.text(" "),
            _ => {
                // Only when it's the innermost, as HTML is often
                // left unclosed.
                if matches!(self.open.last(), Some(o) if o.kind.is_like(&Kind::Html(name.to_owned())))
                {
                    self.open.pop();
                }
                if name.len() == 2 && name.starts_with('h') {
                    self.writer.hard_break();
                }
            }
        }
    }

    /// Draws the text of an HTML `<pre>` like a code block.
    pub(super) fn pre(&mut self, text: &str) {
        // Like a browser, skip one newline right after `<pre>`.
        let text = text.strip_prefix('\n').unwrap_or(text);
        self.code_block("", text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_text() {
        let tokens = tokenize(r#"<li><a href="/x?a=1&amp;b=2">Hi &amp; bye</a></li>"#);
        assert_eq!(tokens[1].attr("href"), Some("/x?a=1&b=2"));
        assert_eq!(tokens[2], Token::Text("Hi & bye".into()));
        assert_eq!(tokens[3], Token::Close("a".into()));
    }

    #[test]
    fn skips_svg_content_and_comments() {
        let tokens = tokenize(r#"a<!-- no --><svg aria-label="fig"><text>x</text></svg>b"#);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].attr("aria-label"), Some("fig"));
        assert_eq!(tokens[2], Token::Text("b".into()));
    }

    #[test]
    fn multibyte_text() {
        assert_eq!(
            tokenize("λ<b>"),
            [
                Token::Text("λ".into()),
                Token::Open {
                    name: "b".into(),
                    attrs: vec![]
                }
            ]
        );
    }

    #[test]
    fn stray_angle_brackets_are_text() {
        let tokens = tokenize("1 < 2 > 0");
        let text: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(text, "1 < 2 > 0");
    }
}
