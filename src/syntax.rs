//! Syntax highlighting for code blocks, with syntect.

use std::sync::OnceLock;

use ratatui_core::style::{Color, Style};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::Theme;
use crate::wrap::Piece;

impl Theme {
    /// Colors `code` as the language named `lang`, one entry per line,
    /// with the syntax theme over `base`. Returns `None` when there's no
    /// syntax theme or the language or syntax theme is unknown.
    pub(crate) fn highlight(&self, code: &str, lang: &str, base: Style) -> Option<Vec<Vec<Piece>>> {
        let syntax = syntaxes().find_syntax_by_token(lang)?;
        let colors = themes().themes.get(self.syntax?)?;
        let mut highlighter = HighlightLines::new(syntax, colors);
        let mut lines = Vec::new();
        for line in LinesWithEndings::from(code) {
            let ranges = highlighter.highlight_line(line, syntaxes()).ok()?;
            lines.push(
                ranges
                    .into_iter()
                    .map(|(style, text)| {
                        let c = style.foreground;
                        Piece::new(
                            text.trim_end_matches('\n'),
                            base.fg(Color::Rgb(c.r, c.g, c.b)),
                        )
                    })
                    .filter(|piece| !piece.text.is_empty())
                    .collect(),
            );
        }
        Some(lines)
    }
}

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn themes() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}
