//! Lays inline content out in lines, under the prefixes of the blocks it's
//! in, and keeps track of where links land.

use ratatui_core::style::Style;
use ratatui_core::text::{Line, Span};

use crate::wrap::{Inline, Piece};
use crate::{Link, Region};

pub(crate) struct Writer {
    width: usize,
    lines: Vec<Line<'static>>,
    links: Vec<Link>,
    prefixes: Vec<Prefix>,
    /// Inline content not laid out yet.
    inline: Vec<Inline>,
    /// Whether a blank line is owed before the next block.
    gap: bool,
}

impl Writer {
    pub fn new(width: u16) -> Self {
        Self {
            width: usize::from(width).max(1),
            lines: Vec::new(),
            links: Vec::new(),
            prefixes: Vec::new(),
            inline: Vec::new(),
            gap: false,
        }
    }

    /// Everything written, once the last of the inline content is laid out.
    pub fn finish(mut self) -> (Vec<Line<'static>>, Vec<Link>) {
        self.flush();
        (self.lines, self.links)
    }

    pub fn piece(&mut self, text: impl Into<String>, style: Style, link: Option<usize>) {
        self.inline.push(Inline::Piece(Piece {
            text: text.into(),
            style,
            link,
        }));
    }

    /// Starts a link to `url`, for pieces to point at.
    pub fn link(&mut self, url: &str) -> usize {
        self.links.push(Link {
            url: url.to_owned(),
            regions: Vec::new(),
        });
        self.links.len() - 1
    }

    pub fn line_break(&mut self) {
        self.inline.push(Inline::Break);
    }

    /// Ends the current line, unless nothing but whitespace is on it.
    #[cfg(feature = "html")]
    pub fn hard_break(&mut self) {
        let blank = self
            .inline
            .iter()
            .rev()
            .take_while(|i| matches!(i, Inline::Piece(_)))
            .all(|i| matches!(i, Inline::Piece(p) if p.text.trim().is_empty()));
        if !blank {
            self.line_break();
        }
    }

    /// Lays out any pending inline content.
    pub fn flush(&mut self) {
        while matches!(self.inline.last(), Some(Inline::Break)) {
            self.inline.pop();
        }
        if self.inline.is_empty() {
            return;
        }
        // Every prefix is as wide on its first line as on the rest, so one
        // width holds for the whole block.
        let inline = std::mem::take(&mut self.inline);
        for line in Inline::wrap(&inline, self.available()) {
            self.emit(line);
        }
    }

    /// Ends a block, owing a blank line before the next.
    pub fn end_block(&mut self) {
        self.flush();
        self.gap = true;
    }

    /// Writes one line of pieces after the current prefixes, recording where
    /// its links landed. Anything past the width, which only deeply nested
    /// prefixes in a very narrow width can cause, is cut off.
    pub fn emit(&mut self, pieces: Vec<Piece>) {
        self.pay_gap();
        let mut all = Vec::new();
        for prefix in &mut self.prefixes {
            all.extend(
                if prefix.used {
                    &prefix.rest
                } else {
                    &prefix.first
                }
                .iter()
                .cloned(),
            );
            prefix.used = true;
        }
        all.extend(pieces);
        truncate(&mut all, self.width);

        let line = self.lines.len();
        let mut x = 0;
        for piece in &all {
            let w = piece.width() as u16;
            if let Some(id) = piece.link {
                let regions = &mut self.links[id].regions;
                match regions.last_mut() {
                    Some(r) if r.line == line && r.end == x => r.end += w,
                    _ => regions.push(Region {
                        line,
                        start: x,
                        end: x + w,
                    }),
                }
            }
            x += w;
        }
        let spans: Vec<Span<'static>> = all
            .into_iter()
            .map(|p| Span::styled(p.text, p.style))
            .collect();
        self.lines.push(Line::from(spans));
    }

    /// Writes a blank line, keeping the bars of any block quotes.
    fn blank(&mut self) {
        let mut pieces: Vec<Piece> = self
            .prefixes
            .iter()
            .flat_map(|p| p.rest.iter().cloned())
            .collect();
        truncate(&mut pieces, self.width);
        if let Some(last) = pieces.last_mut() {
            last.text.truncate(last.text.trim_end().len());
        }
        let spans: Vec<Span<'static>> = pieces
            .into_iter()
            .filter(|p| !p.text.is_empty())
            .map(|p| Span::styled(p.text, p.style))
            .collect();
        self.lines.push(Line::from(spans));
    }

    /// Whether a blank line is owed before the next block.
    pub fn set_gap(&mut self, owed: bool) {
        self.gap = owed;
    }

    /// Writes the blank line owed, if there is one, now rather than with the
    /// next block, so it goes outside a prefix about to be pushed.
    pub fn pay_gap(&mut self) {
        if std::mem::take(&mut self.gap) && !self.lines.is_empty() {
            self.blank();
        }
    }

    /// The line the next block starts on, counting a blank line owed before
    /// it.
    pub fn next_block_line(&self) -> usize {
        self.lines.len() + usize::from(self.gap && !self.lines.is_empty())
    }

    pub fn push_prefix(&mut self, prefix: Prefix) {
        self.prefixes.push(prefix);
    }

    pub fn pop_prefix(&mut self) {
        self.prefixes.pop();
    }

    /// How far in content starts, past the prefixes.
    pub fn indent(&self) -> usize {
        self.width - self.available().min(self.width)
    }

    /// Room left for content once the prefixes are drawn.
    pub fn available(&self) -> usize {
        let prefixes: usize = self
            .prefixes
            .iter()
            .map(|p| {
                let pieces = if p.used { &p.rest } else { &p.first };
                pieces.iter().map(Piece::width).sum::<usize>()
            })
            .sum();
        self.width.saturating_sub(prefixes).max(1)
    }

    pub fn inline(&self) -> &[Inline] {
        &self.inline
    }

    /// The inline content so far, taken out to lay out some other way, like
    /// in a table cell.
    pub fn take_inline(&mut self) -> Vec<Inline> {
        std::mem::take(&mut self.inline)
    }

    /// Whether the inline content so far shows nothing.
    #[cfg(feature = "html")]
    pub fn inline_is_blank(&self) -> bool {
        self.inline.iter().all(|i| match i {
            Inline::Piece(p) => p.text.trim().is_empty(),
            Inline::Break => true,
        })
    }

    #[cfg(feature = "html")]
    pub fn lines_written(&self) -> usize {
        self.lines.len()
    }
}

/// Something drawn at the start of every line of a block, like a block
/// quote's bar or a list item's bullet. `first` is used on the block's first
/// line and `rest` after.
pub(crate) struct Prefix {
    pub first: Vec<Piece>,
    pub rest: Vec<Piece>,
    pub used: bool,
}

impl Prefix {
    /// A prefix that's the same on every line.
    pub fn same(piece: Piece) -> Self {
        Self {
            first: vec![piece.clone()],
            rest: vec![piece],
            used: false,
        }
    }

    /// A marker on the first line, like a bullet, and spaces as wide under it.
    pub fn hanging(marker: Piece, text: Style) -> Self {
        let indent = " ".repeat(marker.width());
        Self {
            first: vec![marker],
            rest: vec![Piece::new(indent, text)],
            used: false,
        }
    }
}

/// Cuts pieces off at `width` columns.
fn truncate(pieces: &mut Vec<Piece>, width: usize) {
    let mut used = 0;
    for i in 0..pieces.len() {
        let w = pieces[i].width();
        if used + w > width {
            pieces[i] = pieces[i].split_at_width(width - used).0;
            pieces.truncate(i + 1);
            return;
        }
        used += w;
    }
}
