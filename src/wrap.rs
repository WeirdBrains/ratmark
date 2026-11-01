//! Word wrapping for runs of styled text.

use std::ops::Range;

use ratatui_core::style::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// A run of text in one style, possibly part of a link.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Piece {
    pub text: String,
    pub style: Style,
    pub link: Option<usize>,
}

impl Piece {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            link: None,
        }
    }

    pub fn width(&self) -> usize {
        self.text.width()
    }

    /// The same piece with other text.
    pub fn with_text(&self, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: self.style,
            link: self.link,
        }
    }

    /// Splits the piece after as many whole characters as fit in `columns`.
    pub fn split_at_width(&self, columns: usize) -> (Self, Self) {
        let (head, rest) = self.text.split_at(fitting(&self.text, columns));
        (self.with_text(head), self.with_text(rest))
    }

    /// The part of the piece in `columns`, counted from its start. A wide
    /// character cut by either edge shows as spaces.
    pub fn cut(&self, columns: Range<usize>) -> Self {
        let (before, rest) = self.text.split_at(fitting(&self.text, columns.start));
        let mut at = before.width();
        let mut rest = rest;
        let mut text = String::new();
        if at < columns.start
            && let Some(c) = rest.chars().next()
        {
            let to = at + c.width().unwrap_or(0);
            let shown = to.min(columns.end).saturating_sub(columns.start);
            text.push_str(&" ".repeat(shown));
            at = to;
            rest = &rest[c.len_utf8()..];
        }
        let (inside, after) = rest.split_at(fitting(rest, columns.end.saturating_sub(at)));
        text.push_str(inside);
        at += inside.width();
        if !after.is_empty() && at < columns.end {
            text.push_str(&" ".repeat(columns.end - at));
        }
        self.with_text(text)
    }
}

/// How many bytes of `text` fit in `columns`, in whole characters.
fn fitting(text: &str, columns: usize) -> usize {
    let mut used = 0;
    for (i, c) in text.char_indices() {
        used += c.width().unwrap_or(0);
        if used > columns {
            return i;
        }
    }
    text.len()
}

/// Inline content waiting to be laid out.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Inline {
    Piece(Piece),
    /// A hard line break.
    Break,
}

enum Token {
    Word(Vec<Piece>),
    Space(Piece),
    Break,
}

/// Splits inline content into words, spaces and breaks. Runs of whitespace
/// collapse to a single space, like HTML.
fn tokenize(inline: &[Inline]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut word: Vec<Piece> = Vec::new();
    for item in inline {
        let piece = match item {
            Inline::Piece(piece) => piece,
            Inline::Break => {
                if !word.is_empty() {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                }
                tokens.push(Token::Break);
                continue;
            }
        };
        let mut rest = piece.text.as_str();
        while !rest.is_empty() {
            let split = rest.find(char::is_whitespace).unwrap_or(rest.len());
            if split > 0 {
                word.push(Piece {
                    text: rest[..split].to_owned(),
                    ..piece.clone()
                });
            }
            rest = &rest[split..];
            let spaces = rest.len() - rest.trim_start().len();
            if spaces > 0 {
                if !word.is_empty() {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                }
                if !matches!(tokens.last(), Some(Token::Space(_))) {
                    tokens.push(Token::Space(Piece {
                        text: " ".into(),
                        ..piece.clone()
                    }));
                }
                rest = &rest[spaces..];
            }
        }
    }
    if !word.is_empty() {
        tokens.push(Token::Word(word));
    }
    tokens
}

/// Lays inline content out in lines at most `width` columns wide. Words wider
/// than a whole line are split between characters.
pub(crate) fn wrap(inline: &[Inline], width: usize) -> Vec<Vec<Piece>> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line: Vec<Piece> = Vec::new();
    let mut used = 0;
    let mut space: Option<Piece> = None;

    for token in tokenize(inline) {
        match token {
            Token::Break => {
                lines.push(std::mem::take(&mut line));
                used = 0;
                space = None;
            }
            Token::Space(piece) => space = Some(piece),
            Token::Word(pieces) => {
                let word: usize = pieces.iter().map(Piece::width).sum();
                if used > 0 {
                    if used + 1 + word <= width {
                        line.extend(space.take());
                        used += 1;
                    } else {
                        lines.push(std::mem::take(&mut line));
                        used = 0;
                    }
                }
                space = None;
                for mut piece in pieces {
                    while used + piece.width() > width {
                        // Too long for any line, so break it wherever it
                        // fills up.
                        let (mut head, mut rest) = piece.split_at_width(width.saturating_sub(used));
                        if head.text.is_empty() {
                            if used > 0 {
                                lines.push(std::mem::take(&mut line));
                                used = 0;
                                continue;
                            }
                            // A character wider than a whole line gets a
                            // line of its own.
                            let len = piece.text.chars().next().map_or(0, char::len_utf8);
                            (head, rest) = (
                                piece.with_text(&piece.text[..len]),
                                piece.with_text(&piece.text[len..]),
                            );
                        }
                        used += head.width();
                        line.push(head);
                        piece = rest;
                    }
                    used += piece.width();
                    if !piece.text.is_empty() {
                        line.push(piece);
                    }
                }
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Vec<Piece>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|p| p.text.as_str()).collect())
            .collect()
    }

    fn plain(s: &str) -> Vec<Inline> {
        vec![Inline::Piece(Piece::new(s, Style::new()))]
    }

    #[test]
    fn wraps_at_spaces() {
        let lines = wrap(&plain("the quick brown fox jumps"), 10);
        assert_eq!(text(&lines), ["the quick", "brown fox", "jumps"]);
    }

    #[test]
    fn collapses_whitespace() {
        let lines = wrap(&plain("  a   b \t c  "), 80);
        assert_eq!(text(&lines), ["a b c"]);
    }

    #[test]
    fn splits_long_words() {
        let lines = wrap(&plain("abcdefghij xy"), 4);
        assert_eq!(text(&lines), ["abcd", "efgh", "ij", "xy"]);
    }

    #[test]
    fn keeps_styled_runs_in_one_word() {
        let bold = Style::new().bold();
        let inline = vec![
            Inline::Piece(Piece::new("foo", Style::new())),
            Inline::Piece(Piece::new("bar baz", bold)),
        ];
        let lines = wrap(&inline, 6);
        assert_eq!(text(&lines), ["foobar", "baz"]);
        assert_eq!(lines[0][1].style, bold);
    }

    #[test]
    fn splits_wide_characters_whole() {
        let lines = wrap(&plain("日本語"), 5);
        assert_eq!(text(&lines), ["日本", "語"]);
        // Too wide for any line, a character gets one of its own.
        let lines = wrap(&plain("a日b"), 1);
        assert_eq!(text(&lines), ["a", "日", "b"]);
    }

    #[test]
    fn cuts_columns() {
        let piece = Piece::new("a日本b", Style::new());
        assert_eq!(piece.split_at_width(2).0.text, "a");
        assert_eq!(piece.cut(1..5).text, "日本");
        // Wide characters cut by an edge show as spaces.
        assert_eq!(piece.cut(2..4).text, "  ");
        assert_eq!(piece.cut(2..6).text, " 本b");
        assert_eq!(piece.cut(0..2).text, "a ");
    }

    #[test]
    fn breaks() {
        let mut inline = plain("a");
        inline.push(Inline::Break);
        inline.extend(plain("b"));
        assert_eq!(text(&wrap(&inline, 80)), ["a", "b"]);
    }
}
