//! Tables, gathered cell by cell and laid out once they're complete.

use pulldown_cmark::Alignment;
use unicode_width::UnicodeWidthStr;

use crate::Theme;
use crate::wrap::{Inline, Piece};

/// Columns are never narrowed past this to make a table fit.
const MIN_WIDTH: usize = 3;

/// Drawn between columns.
const SEPARATOR: &str = " │ ";

/// A cell's inline content, laid out once the column widths are known.
type Cell = Vec<Inline>;

pub(crate) struct Table {
    alignments: Vec<Alignment>,
    /// Each row's cells, the header first. Cells go in the last row.
    rows: Vec<Vec<Cell>>,
}

impl Table {
    pub fn new(alignments: Vec<Alignment>) -> Self {
        Self {
            alignments,
            rows: Vec::new(),
        }
    }

    pub fn start_row(&mut self) {
        self.rows.push(Vec::new());
    }

    pub fn push_cell(&mut self, cell: Cell) {
        if let Some(row) = self.rows.last_mut() {
            row.push(cell);
        }
    }

    /// The table's lines, in `available` columns, with a rule under the
    /// header and, if the theme asks, between every row.
    pub fn lines(&self, available: usize, theme: &Theme) -> Vec<Vec<Piece>> {
        let widths = self.widths(available);
        if widths.is_empty() {
            return Vec::new();
        }
        let mut lines = Vec::new();
        for (r, row) in self.rows.iter().enumerate() {
            lines.extend(self.row_lines(row, &widths, theme));
            let last = r + 1 == self.rows.len();
            if !last && (r == 0 || theme.table_row_rules) {
                let rule = widths
                    .iter()
                    .map(|w| "─".repeat(*w))
                    .collect::<Vec<_>>()
                    .join("─┼─");
                lines.push(vec![Piece::new(rule, theme.table_border)]);
            }
        }
        lines
    }

    /// Each column's width: as wide as its widest cell, then narrowed, widest
    /// first, until the table fits in `available` columns.
    fn widths(&self, available: usize) -> Vec<usize> {
        let columns = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut widths = vec![1; columns];
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(natural_width(cell));
            }
        }
        let separators = SEPARATOR.width() * columns.saturating_sub(1);
        while widths.iter().sum::<usize>() + separators > available {
            let Some((i, &widest)) = widths.iter().enumerate().max_by_key(|(_, w)| **w) else {
                break;
            };
            if widest <= MIN_WIDTH {
                break;
            }
            widths[i] -= 1;
        }
        widths
    }

    /// One row's lines, each cell wrapped to its column and aligned in it.
    fn row_lines(&self, row: &[Cell], widths: &[usize], theme: &Theme) -> Vec<Vec<Piece>> {
        let cells: Vec<Vec<Vec<Piece>>> = widths
            .iter()
            .enumerate()
            .map(|(i, &width)| {
                row.get(i)
                    .map(|cell| Inline::wrap(cell, width))
                    .unwrap_or_default()
            })
            .collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(0).max(1);
        (0..height)
            .map(|y| {
                let mut pieces = Vec::new();
                for (i, cell) in cells.iter().enumerate() {
                    if i > 0 {
                        pieces.push(Piece::new(SEPARATOR, theme.table_border));
                    }
                    let line = cell.get(y).cloned().unwrap_or_default();
                    let used: usize = line.iter().map(Piece::width).sum();
                    let pad = widths[i].saturating_sub(used);
                    let (left, right) = match self.alignments.get(i) {
                        Some(Alignment::Right) => (pad, 0),
                        Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                        _ => (0, pad),
                    };
                    pieces.push(Piece::new(" ".repeat(left), theme.text));
                    pieces.extend(line);
                    pieces.push(Piece::new(" ".repeat(right), theme.text));
                }
                pieces
            })
            .collect()
    }
}

/// How wide a cell is on one line.
fn natural_width(cell: &[Inline]) -> usize {
    Inline::wrap(cell, usize::MAX / 2)
        .iter()
        .map(|l| l.iter().map(Piece::width).sum::<usize>())
        .max()
        .unwrap_or(0)
}
