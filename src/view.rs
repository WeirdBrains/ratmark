//! A scrolling document view, with links that can be focused and clicked.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};
use ratatui_core::style::{Modifier, Style};
use ratatui_core::widgets::StatefulWidget;

use crate::{Heading, Link, Overflow, Region, Rendered, Theme};

/// Draws a [`MarkdownState`]'s document, laid out to fit the area it's
/// given, scrolled and with its focused link highlighted.
///
/// ```
/// use ratmark::{MarkdownState, MarkdownView, Theme};
/// # use ratatui_core::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
///
/// let mut state = MarkdownState::new("See [the docs](https://docs.rs).");
/// let theme = Theme::default();
/// # let area = Rect::new(0, 0, 40, 5);
/// # let mut buf = Buffer::empty(area);
/// MarkdownView::new(&theme).render(area, &mut buf, &mut state);
/// // frame.render_stateful_widget(MarkdownView::new(&theme), area, &mut state);
///
/// state.focus_next();
/// assert_eq!(state.focused().map(|l| l.url.as_str()), Some("https://docs.rs"));
/// assert_eq!(state.link_at(4, 0).map(|l| l.url.as_str()), Some("https://docs.rs"));
/// ```
#[derive(Debug, Clone)]
pub struct MarkdownView<'t> {
    theme: &'t Theme,
    focus: Style,
}

impl<'t> MarkdownView<'t> {
    /// A view drawing with `theme`.
    pub fn new(theme: &'t Theme) -> Self {
        Self {
            theme,
            focus: Style::new().add_modifier(Modifier::REVERSED),
        }
    }

    /// The style patched over the focused link. Reversed colors by default.
    pub fn focus_style(mut self, style: Style) -> Self {
        self.focus = style;
        self
    }
}

impl StatefulWidget for MarkdownView<'_> {
    type State = MarkdownState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut MarkdownState) {
        state.lay_out(area, self.theme);
        state.draw_lines(buf);
        state.draw_shifted_code(buf);
        #[cfg(feature = "links")]
        state.draw_links(buf);
        state.draw_focus(buf, self.focus);
    }
}

/// A document and where it's scrolled to, for [`MarkdownView`]. Scrolling
/// stays inside the document as last drawn.
///
/// ```
/// use ratmark::{MarkdownState, MarkdownView, Theme};
/// # use ratatui_core::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
///
/// let markdown = "# Intro\n\na\n\nb\n\n## The End\n\n```\nlet wide = 1;\n```\n";
/// let mut state = MarkdownState::new(markdown);
/// # let area = Rect::new(0, 0, 10, 3);
/// # let mut buf = Buffer::empty(area);
/// MarkdownView::new(&Theme::default()).render(area, &mut buf, &mut state);
///
/// // Scrolling says whether it moved, which it doesn't at either end.
/// assert!(state.scroll_by(1));
/// assert!(state.scroll_by(-1));
/// assert!(!state.scroll_by(-1));
///
/// // A `#section` link scrolls to the heading it loosely names.
/// state.scroll_to_anchor("the-end");
/// assert_eq!(state.scroll(), 6);
///
/// // Code too wide for the page shifts sideways, the topmost block on
/// // screen here, or the one under the pointer with `shift_at`.
/// assert!(state.shift_by(2));
/// assert_eq!(state.shift(0), 2);
/// ```
#[derive(Debug, Clone, Default)]
pub struct MarkdownState {
    markdown: String,
    /// The layout, and the width it was made for.
    rendered: Option<(u16, Rendered)>,
    scroll: usize,
    /// How far each code block too wide for the page is shifted sideways,
    /// by [`Overflow::block`].
    shifts: Vec<usize>,
    focus: Option<usize>,
    /// Where the view was last drawn.
    area: Rect,
    /// A `#section` to scroll to once laid out.
    anchor: Option<String>,
    /// The document's own address, to make its links absolute.
    #[cfg(feature = "links")]
    base: Option<url::Url>,
}

impl MarkdownState {
    /// The state of a view showing `markdown`, at the top.
    pub fn new(markdown: impl Into<String>) -> Self {
        Self {
            markdown: markdown.into(),
            ..Self::default()
        }
    }

    /// Sets the document's own address, so the links its cells carry are
    /// absolute and work outside the page, like in a terminal. An address
    /// that isn't a valid absolute URL is ignored, leaving links as written.
    #[cfg(feature = "links")]
    pub fn with_link_base(mut self, url: &str) -> Self {
        self.base = url::Url::parse(url).ok();
        self
    }

    /// The Markdown being shown.
    pub fn markdown(&self) -> &str {
        &self.markdown
    }

    /// The document as last laid out, once it has been drawn.
    pub fn rendered(&self) -> Option<&Rendered> {
        self.rendered.as_ref().map(|(_, r)| r)
    }

    /// The first line on screen.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Scrolls so `line` is at the top, as far as the document allows.
    pub fn scroll_to(&mut self, line: usize) {
        self.scroll = line;
        self.clamp();
    }

    /// Scrolls by `lines`, down when positive. Returns whether it moved,
    /// which it doesn't at either end.
    pub fn scroll_by(&mut self, lines: isize) -> bool {
        let was = self.scroll;
        self.scroll_to(self.scroll.saturating_add_signed(lines));
        self.scroll != was
    }

    /// Scrolls by `pages` screenfuls, less a couple of lines each for
    /// context, down when positive. Returns whether it moved, which it
    /// doesn't at either end.
    pub fn page_by(&mut self, pages: isize) -> bool {
        let page = (self.area.height as isize - 2).max(1);
        self.scroll_by(pages * page)
    }

    /// Scrolls to the heading a `#section` link points at, now or once the
    /// document is laid out. Matching ignores case and punctuation, since
    /// tools make heading ids a little differently.
    pub fn scroll_to_anchor(&mut self, anchor: &str) {
        self.anchor = Some(anchor.to_owned());
        self.apply_anchor();
    }

    /// How far the `block`th code block is shifted to the left, in columns.
    pub fn shift(&self, block: usize) -> usize {
        self.shifts.get(block).copied().unwrap_or(0)
    }

    /// Shifts the topmost wide code block on screen by `columns`, to the
    /// left when positive, as far as its widest line allows.
    /// Returns whether it moved, which it doesn't at its end.
    pub fn shift_by(&mut self, columns: isize) -> bool {
        // TODO: Let tab focus wide code blocks along with links, and shift
        // the focused one.
        let block = self
            .visible_overflows()
            .next()
            .map(|o| (o.block, o.max_shift()));
        block.is_some_and(|(block, max)| self.shift_block(block, max, columns))
    }

    /// Shifts the wide code block drawn at screen position `x`, `y` by
    /// `columns`, if there is one there. Returns whether it moved, which it
    /// doesn't when there's none or it's at its end.
    pub fn shift_at(&mut self, x: u16, y: u16, columns: isize) -> bool {
        let Some((line, x)) = self.document_position(x, y) else {
            return false;
        };
        let block = self
            .visible_overflows()
            .find(|o| o.lines.contains(&line) && o.columns().contains(&x))
            .map(|o| (o.block, o.max_shift()));
        block.is_some_and(|(block, max)| self.shift_block(block, max, columns))
    }

    /// The focused link.
    pub fn focused(&self) -> Option<&Link> {
        self.rendered()?.links.get(self.focus?)
    }

    /// Focuses the next link, starting from the top of the screen, and
    /// scrolls to it.
    pub fn focus_next(&mut self) {
        self.move_focus(true);
    }

    /// Focuses the previous link, starting from the bottom of the screen,
    /// and scrolls to it.
    pub fn focus_prev(&mut self) {
        self.move_focus(false);
    }

    /// Focuses no link.
    pub fn clear_focus(&mut self) {
        self.focus = None;
    }

    /// The link drawn at screen position `x`, `y`, if any.
    pub fn link_at(&self, x: u16, y: u16) -> Option<&Link> {
        let (line, x) = self.document_position(x, y)?;
        self.rendered()?.link_at(line, x)
    }

    /// Lays the document out for `area`, if it isn't already for its
    /// width, and keeps the scroll inside it.
    fn lay_out(&mut self, area: Rect, theme: &Theme) {
        self.area = area;
        if self.rendered.as_ref().is_none_or(|(w, _)| *w != area.width) {
            // Links come out in the same order at any width, so the focus
            // stays put through a resize.
            let rendered = Rendered::render(&self.markdown, area.width, theme);
            self.rendered = Some((area.width, rendered));
        }
        self.apply_anchor();
        self.clamp();
    }

    /// Draws the lines on screen, as laid out.
    fn draw_lines(&self, buf: &mut Buffer) {
        let Some(rendered) = self.rendered() else {
            return;
        };
        let area = self.area;
        let lines = rendered
            .lines
            .iter()
            .skip(self.scroll)
            .take(usize::from(area.height));
        for (y, line) in (area.y..).zip(lines) {
            buf.set_line(area.x, y, line, area.width);
        }
    }

    /// Draws the wide code blocks shifted sideways over their lines.
    fn draw_shifted_code(&self, buf: &mut Buffer) {
        let area = self.area;
        for overflow in self.visible_overflows() {
            let shift = self.shift(overflow.block);
            if shift == 0 {
                continue;
            }
            let x = area.x + overflow.x;
            let width = area.right().saturating_sub(x);
            for line in overflow.lines.clone() {
                let Some(row) = line
                    .checked_sub(self.scroll)
                    .filter(|r| *r < usize::from(area.height))
                else {
                    continue;
                };
                let shifted = overflow.row(line - overflow.lines.start, shift);
                buf.set_line(x, area.y + row as u16, &shifted, width);
            }
        }
    }

    /// Marks the cells of each link with where it goes, for backends that
    /// draw real links.
    #[cfg(feature = "links")]
    fn draw_links(&self, buf: &mut Buffer) {
        let Some(rendered) = self.rendered() else {
            return;
        };
        for (i, link) in rendered.links.iter().enumerate() {
            // One id per link, so a terminal treats the pieces of a wrapped
            // link as one.
            let url = self.absolute(&link.url);
            let hyperlink =
                ratatui_core::buffer::Hyperlink::new(&url).with_id(format!("ratmark-{i}"));
            for rect in link
                .regions
                .iter()
                .filter_map(|r| r.on_screen(self.scroll, self.area))
            {
                buf.set_hyperlink(rect, Some(&hyperlink));
            }
        }
    }

    /// Patches `style` over the focused link.
    fn draw_focus(&self, buf: &mut Buffer, style: Style) {
        let Some(link) = self.focused() else {
            return;
        };
        for rect in link
            .regions
            .iter()
            .filter_map(|r| r.on_screen(self.scroll, self.area))
        {
            buf.set_style(rect, style);
        }
    }

    /// The document's line and column at screen position `x`, `y`, if it's
    /// in the view.
    fn document_position(&self, x: u16, y: u16) -> Option<(usize, u16)> {
        let area = self.area;
        area.contains(Position::new(x, y))
            .then(|| (self.scroll + usize::from(y - area.y), x - area.x))
    }

    fn shift_block(&mut self, block: usize, max: usize, columns: isize) -> bool {
        if self.shifts.len() <= block {
            self.shifts.resize(block + 1, 0);
        }
        let shift = &mut self.shifts[block];
        let was = *shift;
        *shift = shift.saturating_add_signed(columns).min(max);
        *shift != was
    }

    /// The code blocks too wide for the page that are on screen.
    fn visible_overflows(&self) -> impl Iterator<Item = &Overflow> {
        let screen = self.scroll..self.scroll + usize::from(self.area.height);
        self.rendered()
            .into_iter()
            .flat_map(|r| &r.overflows)
            .filter(move |o| o.lines.start < screen.end && screen.start < o.lines.end)
    }

    /// How far down the document can scroll.
    fn max_scroll(&self) -> usize {
        let lines = self.rendered().map_or(0, |r| r.lines.len());
        lines.saturating_sub(usize::from(self.area.height))
    }

    fn clamp(&mut self) {
        if self.rendered.is_some() {
            self.scroll = self.scroll.min(self.max_scroll());
        }
    }

    fn apply_anchor(&mut self) {
        let Some((_, rendered)) = &self.rendered else {
            return;
        };
        let Some(anchor) = self.anchor.take() else {
            return;
        };
        if let Some(heading) = rendered.headings.iter().find(|h| h.matches_anchor(&anchor)) {
            self.scroll_to(heading.line);
        }
    }

    fn move_focus(&mut self, forward: bool) {
        let height = usize::from(self.area.height.max(1));
        let scroll = self.scroll;
        let Some(rendered) = self.rendered() else {
            return;
        };
        let count = rendered.links.len();
        if count == 0 {
            return;
        }
        let line = |i: usize| rendered.links[i].regions.first().map_or(0, |r| r.line);
        let visible = |i: usize| (scroll..scroll + height).contains(&line(i));
        let next = match self.focus {
            // Keep going from a focused link that's still on screen.
            Some(i) if visible(i) => {
                if forward {
                    (i + 1) % count
                } else {
                    (i + count - 1) % count
                }
            }
            // Otherwise start from what's on screen, so a long page doesn't
            // jump back to the top.
            _ => {
                let on_screen = (0..count).filter(|&i| visible(i));
                let found = if forward {
                    on_screen.min()
                } else {
                    on_screen.max()
                };
                found.unwrap_or_else(|| {
                    let below = (0..count).find(|&i| line(i) >= scroll + height);
                    let above = (0..count).rev().find(|&i| line(i) < scroll);
                    if forward {
                        below.unwrap_or(0)
                    } else {
                        above.unwrap_or(count - 1)
                    }
                })
            }
        };
        let at = line(next);
        self.focus = Some(next);
        if at < self.scroll {
            self.scroll_to(at);
        } else if at >= self.scroll + height {
            self.scroll_to(at + 1 - height);
        }
    }

    /// `url` resolved against the document's own address, if it has one. A
    /// link that can't be resolved is left as written.
    #[cfg(feature = "links")]
    fn absolute(&self, url: &str) -> String {
        match &self.base {
            Some(base) => base.join(url).map_or_else(|_| url.to_owned(), String::from),
            None => url.to_owned(),
        }
    }
}

impl Region {
    /// Where the region is on screen in `area`, scrolled to `scroll`, if it's
    /// in view.
    fn on_screen(&self, scroll: usize, area: Rect) -> Option<Rect> {
        let row = self
            .line
            .checked_sub(scroll)
            .filter(|r| *r < usize::from(area.height))?;
        let end = self.end.min(area.width);
        let start = self.start.min(end);
        Some(Rect::new(
            area.x + start,
            area.y + row as u16,
            end - start,
            1,
        ))
    }
}

impl Heading {
    /// Whether a `#section` link loosely names this heading.
    fn matches_anchor(&self, anchor: &str) -> bool {
        let simplify = |s: &str| -> String {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect()
        };
        let anchor = simplify(anchor);
        !anchor.is_empty() && simplify(&self.text) == anchor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(state: &mut MarkdownState, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        MarkdownView::new(&Theme::plain()).render(area, &mut buf, state);
        buf
    }

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    #[test]
    fn scrolls_within_the_document() {
        let mut state = MarkdownState::new("a\n\nb\n\nc");
        draw(&mut state, 10, 2);
        assert!(state.scroll_by(100));
        assert!(!state.scroll_by(1));
        assert_eq!(state.scroll(), 3);
        let buf = draw(&mut state, 10, 2);
        assert_eq!((row(&buf, 0), row(&buf, 1)), ("".into(), "c".into()));
        state.scroll_by(-100);
        assert_eq!(state.scroll(), 0);
        // Two lines high, a page keeps none for context, but still moves.
        assert!(state.page_by(2));
        assert_eq!(state.scroll(), 2);
        assert!(state.page_by(-5));
        assert_eq!(state.scroll(), 0);
    }

    #[test]
    fn focus_follows_the_screen() {
        let md = "[a](/a)\n\n[b](/b)\n\nx\n\nx\n\n[c](/c)";
        let mut state = MarkdownState::new(md);
        draw(&mut state, 10, 3);
        state.focus_next();
        assert_eq!(state.focused().unwrap().url, "/a");
        state.scroll_to(6);
        state.focus_next();
        assert_eq!(state.focused().unwrap().url, "/c");
        state.focus_next();
        assert_eq!(state.focused().unwrap().url, "/a");
        assert_eq!(state.scroll(), 0);
    }

    #[test]
    fn focused_link_is_highlighted() {
        let mut state = MarkdownState::new("go [here](/x)");
        draw(&mut state, 20, 1);
        state.focus_next();
        let buf = draw(&mut state, 20, 1);
        assert!(buf[(3, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buf[(2, 0)].modifier.contains(Modifier::REVERSED));
    }

    #[cfg(feature = "links")]
    #[test]
    fn cells_carry_links() {
        let mut state = MarkdownState::new("go [over there](/x) now");
        let buf = draw(&mut state, 10, 2);
        assert_eq!(buf[(3, 0)].hyperlink().map(|h| h.uri()), Some("/x"));
        assert_eq!(
            buf[(0, 1)].hyperlink().map(|h| h.id()),
            Some(Some("ratmark-0"))
        );
        assert_eq!(buf[(2, 0)].hyperlink(), None);
    }

    #[cfg(feature = "links")]
    #[test]
    fn absolute_links() {
        let base = "https://a.com/x/page";
        let state = MarkdownState::new("").with_link_base(base);
        assert_eq!(state.absolute("/root"), "https://a.com/root");
        assert_eq!(state.absolute("sib"), "https://a.com/x/sib");
        assert_eq!(state.absolute("../up"), "https://a.com/up");
        assert_eq!(state.absolute("#s"), "https://a.com/x/page#s");
        assert_eq!(state.absolute("//c.com/y"), "https://c.com/y");
        assert_eq!(state.absolute("mailto:a@b.c"), "mailto:a@b.c");
        let root = MarkdownState::new("").with_link_base("https://a.com");
        assert_eq!(root.absolute("p"), "https://a.com/p");
        let mut state = MarkdownState::new("[x](y)").with_link_base(base);
        let buf = draw(&mut state, 10, 1);
        assert_eq!(
            buf[(0, 0)].hyperlink().map(|h| h.uri()),
            Some("https://a.com/x/y")
        );
    }

    #[test]
    fn clicks_find_links() {
        let mut state = MarkdownState::new("go [here](/x)");
        let area = Rect::new(5, 2, 20, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 5));
        MarkdownView::new(&Theme::plain()).render(area, &mut buf, &mut state);
        assert_eq!(state.link_at(8, 2).map(|l| l.url.as_str()), Some("/x"));
        assert!(state.link_at(5, 2).is_none());
        assert!(state.link_at(8, 3).is_none());
    }

    #[test]
    fn wide_code_shifts_one_block_at_a_time() {
        let mut state = MarkdownState::new("```\nabcdefgh\n```\n\n```\nijklmnop\n```\n\na");
        draw(&mut state, 6, 3);
        assert!(state.shift_by(2));
        let buf = draw(&mut state, 6, 3);
        assert_eq!(
            (row(&buf, 0), row(&buf, 2)),
            ("‹cdef›".into(), " ijkl›".into())
        );
        assert!(state.shift_by(100));
        assert!(!state.shift_by(1));
        assert_eq!((state.shift(0), state.shift(1)), (4, 0));
        assert!(state.shift_at(3, 2, 1));
        assert!(!state.shift_at(3, 1, 1));
        assert_eq!((state.shift(0), state.shift(1)), (4, 1));
        // Past the first block, the keys shift the second.
        state.scroll_to(2);
        state.shift_by(1);
        assert_eq!((state.shift(0), state.shift(1)), (4, 2));
    }

    #[test]
    fn anchors() {
        let mut state = MarkdownState::new("a\n\nb\n\n## The End\n\nc");
        state.scroll_to_anchor("the-end");
        draw(&mut state, 20, 2);
        assert_eq!(state.scroll(), 4);
    }
}
