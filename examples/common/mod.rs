//! Prints a document to the terminal with [`MarkdownView`], drawn by
//! Ratatui's crossterm backend above a one-line inline viewport, which is
//! left empty for the shell's prompt.

use std::io;

use ratatui_core::terminal::{Terminal, TerminalOptions, Viewport};
use ratatui_core::widgets::StatefulWidget;
use ratatui_crossterm::CrosstermBackend;
use ratmark::{MarkdownState, MarkdownView, Rendered, Theme};

/// How wide the examples draw, at most.
pub const WIDTH: u16 = 60;

/// Draws the whole of `state`'s document, `WIDTH` columns wide or the
/// terminal's width if that's narrower.
pub fn print(state: &mut MarkdownState, theme: &Theme) -> io::Result<()> {
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(io::stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(1),
        },
    )?;
    let width = terminal.size()?.width.min(WIDTH);
    let height = Rendered::render(state.markdown(), width, theme).lines.len() as u16;
    terminal.insert_before(height, |buf| {
        let mut area = buf.area;
        area.width = width;
        MarkdownView::new(theme).render(area, buf, state);
    })?;
    let below = terminal.get_frame().area().as_position();
    terminal.set_cursor_position(below)
}
