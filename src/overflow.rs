//! Code blocks too wide for the page, drawn shifted sideways.

use std::ops::Range;

use ratatui_core::style::Style;
use ratatui_core::text::{Line, Span};

use crate::wrap::Piece;

/// A code block too wide for the page. Its lines are cut off at the page's
/// edge, with a `›` where there's more, and can be drawn shifted sideways to
/// show the rest.
#[derive(Debug, Clone, PartialEq)]
pub struct Overflow {
    /// Which of the document's code blocks this is, counting from 0, the
    /// same at any width.
    pub block: usize,
    /// The lines the block is drawn on.
    pub lines: Range<usize>,
    /// The column the block starts at, including its padding, if any.
    pub x: u16,
    /// How many columns the code gets, not counting the padding.
    pub width: u16,
    /// The code, line by line, uncut.
    pub(crate) code: Vec<Vec<Piece>>,
    pub(crate) style: Style,
    /// Whether there's a column of padding on either side, from
    /// [`Theme::code_block_padding`].
    pub(crate) padded: bool,
}

impl Overflow {
    /// How far the block can be shifted before its widest line runs out.
    pub fn max_shift(&self) -> usize {
        let widest = self
            .code
            .iter()
            .map(|line| width_of(line))
            .max()
            .unwrap_or(0);
        widest.saturating_sub(usize::from(self.width))
    }

    /// The columns the block is drawn across, padding included.
    pub(crate) fn columns(&self) -> Range<u16> {
        self.x..self.x + self.width + if self.padded { 2 } else { 0 }
    }

    /// The block's `i`th line with its padding, shifted `shift` columns to
    /// the left, `‹` and `›` showing where code is out of sight.
    pub fn row(&self, i: usize, shift: usize) -> Line<'static> {
        let spans = self.pieces(i, shift);
        Line::from_iter(spans.into_iter().map(|p| Span::styled(p.text, p.style)))
    }

    /// The pieces of [`Overflow::row`].
    pub(crate) fn pieces(&self, i: usize, shift: usize) -> Vec<Piece> {
        let shift = shift.min(self.max_shift());
        let width = usize::from(self.width);
        let code = &self.code[i];
        let code_width = width_of(code);
        let after = code_width > shift + width;
        // Unpadded, the marks take the code's first and last columns, so
        // leave the code at least one.
        let before = shift > 0 && code_width > 0 && (self.padded || width > usize::from(after));
        let (start, end) = if self.padded {
            (shift, shift + width)
        } else {
            (
                shift + usize::from(before),
                shift + width - usize::from(after),
            )
        };
        let marker =
            |more: bool, c: &'static str| Piece::new(if more { c } else { " " }, self.style);
        let mut pieces = Vec::new();
        if self.padded || before {
            pieces.push(marker(before, "‹"));
        }
        let mut x = 0;
        for piece in code {
            let from = x;
            x += piece.width();
            if x <= start || from >= end {
                continue;
            }
            let shown = piece.cut(start.saturating_sub(from)..end - from);
            if !shown.text.is_empty() {
                pieces.push(Piece::new(shown.text, self.style.patch(piece.style)));
            }
        }
        let used = x.clamp(start, end) - start;
        pieces.push(Piece::new(" ".repeat(end - start - used), self.style));
        if self.padded || after {
            pieces.push(marker(after, "›"));
        }
        pieces
    }
}

/// How wide a line of pieces is.
fn width_of(line: &[Piece]) -> usize {
    line.iter().map(Piece::width).sum()
}
