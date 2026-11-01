use ratatui_core::style::{Color, Modifier, Style};

/// The styles used for each kind of Markdown element.
///
/// Inline styles like [`Theme::strong`] are patched over the style of the
/// block they appear in, so a theme only needs to set what changes.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub text: Style,
    /// Heading text, by level from `#` to `######`.
    pub headings: [Style; 6],
    /// The style of `#` marks drawn in front of each heading, Markdown or
    /// HTML, to show its level. `None`, the default, draws no marks.
    pub heading_marker: Option<Style>,
    pub emphasis: Style,
    pub strong: Style,
    pub strikethrough: Style,
    pub link: Style,
    pub image: Style,
    pub code: Style,
    /// Fenced and indented code. The background fills the block's width.
    pub code_block: Style,
    /// Leave a column on either side of each code block, where `‹` and `›`
    /// show code cut off at the edges. Without it the code takes the full
    /// width, and the marks are drawn over its first and last columns.
    pub code_block_padding: bool,
    pub quote: Style,
    /// The bar down the side of a block quote.
    pub quote_marker: Style,
    pub list_marker: Style,
    pub rule: Style,
    pub math: Style,
    pub footnote: Style,
    pub table_border: Style,
    pub table_header: Style,
    /// Draw a rule between body rows too, not just under the header.
    pub table_row_rules: bool,
    /// Text from `<small>` tags, used for things like dates under a title.
    pub small: Style,
    /// The syntect theme used to color code blocks, when the `syntax`
    /// feature is on. Only its foreground colors are used.
    pub syntax: Option<&'static str>,
    /// Styles for elements with these classes, written like CSS class
    /// selectors, `.note` or `.note.warning`, and patched in order over
    /// everything inside a matching element. A code block inside takes its
    /// background. Classes come from HTML, with the `html` feature, and from
    /// a kramdown attribute list right after inline code, like
    /// `` `x`{:.new} ``, with the `kramdown` feature.
    pub classes: Vec<(String, Style)>,
}

impl Default for Theme {
    fn default() -> Self {
        Self::gruvbox()
    }
}

impl Theme {
    /// The style for an element with `classes`, from [`Theme::classes`],
    /// or `None` when no selector matches.
    #[cfg(any(feature = "html", feature = "kramdown"))]
    pub(crate) fn class_style<'c>(
        &self,
        classes: impl IntoIterator<Item = &'c str> + Clone,
    ) -> Option<Style> {
        let has = |class: &str| classes.clone().into_iter().any(|c| c == class);
        self.classes
            .iter()
            .filter(|(selector, _)| {
                let mut wanted = selector.split('.').filter(|c| !c.is_empty()).peekable();
                wanted.peek().is_some() && wanted.all(has)
            })
            .map(|(_, style)| *style)
            .reduce(|style, more| style.patch(more))
    }

    /// Gruvbox dark, after the look of Tyler Bloom's blog, The Avid
    /// Rustacean, which this crate's design comes from.
    pub fn gruvbox() -> Self {
        const FG: Color = Color::Rgb(0xeb, 0xdb, 0xb2);
        const BG2: Color = Color::Rgb(0x50, 0x49, 0x45);
        const GRAY: Color = Color::Rgb(0x92, 0x83, 0x74);
        const BURNT: Color = Color::Rgb(0xd6, 0x5d, 0x0e);
        const ORANGE: Color = Color::Rgb(0xfe, 0x80, 0x19);
        const YELLOW: Color = Color::Rgb(0xfa, 0xbd, 0x2f);
        const GREEN: Color = Color::Rgb(0xb8, 0xbb, 0x26);
        const TEAL: Color = Color::Rgb(0x8e, 0xc0, 0x7c);
        const BLUE: Color = Color::Rgb(0x83, 0xa5, 0x98);
        const PINK: Color = Color::Rgb(0xd3, 0x86, 0x9b);

        let bold = Modifier::BOLD;
        Self {
            text: Style::new().fg(FG),
            headings: [
                Style::new()
                    .fg(YELLOW)
                    .add_modifier(bold | Modifier::UNDERLINED),
                Style::new().fg(YELLOW).add_modifier(bold),
                Style::new().fg(ORANGE).add_modifier(bold),
                Style::new().fg(ORANGE),
                Style::new().fg(GREEN),
                Style::new().fg(GREEN),
            ],
            heading_marker: None,
            emphasis: Style::new().add_modifier(Modifier::ITALIC),
            strong: Style::new().add_modifier(bold),
            strikethrough: Style::new().add_modifier(Modifier::CROSSED_OUT),
            link: Style::new().fg(BLUE).add_modifier(Modifier::UNDERLINED),
            image: Style::new().fg(PINK),
            code: Style::new().fg(BURNT).bg(BG2),
            code_block: Style::new().fg(FG).bg(BG2),
            code_block_padding: true,
            quote: Style::new().fg(ORANGE),
            quote_marker: Style::new().fg(GRAY),
            list_marker: Style::new().fg(TEAL),
            rule: Style::new().fg(GRAY),
            math: Style::new().fg(TEAL),
            footnote: Style::new().fg(PINK),
            table_border: Style::new().fg(GRAY),
            table_header: Style::new().fg(YELLOW).add_modifier(bold),
            table_row_rules: true,
            small: Style::new().fg(GRAY),
            syntax: Some("base16-eighties.dark"),
            classes: Vec::new(),
        }
    }

    /// No colors at all, only bold, italic and the like, so the terminal's own
    /// colors show through.
    pub fn plain() -> Self {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let dim = Style::new().add_modifier(Modifier::DIM);
        Self {
            text: Style::new(),
            headings: [
                bold.add_modifier(Modifier::UNDERLINED),
                bold,
                bold,
                bold,
                bold,
                bold,
            ],
            heading_marker: None,
            emphasis: Style::new().add_modifier(Modifier::ITALIC),
            strong: bold,
            strikethrough: Style::new().add_modifier(Modifier::CROSSED_OUT),
            link: Style::new().add_modifier(Modifier::UNDERLINED),
            image: Style::new().add_modifier(Modifier::UNDERLINED),
            code: Style::new().add_modifier(Modifier::REVERSED),
            code_block: Style::new(),
            code_block_padding: true,
            quote: Style::new().add_modifier(Modifier::ITALIC),
            quote_marker: dim,
            list_marker: Style::new(),
            rule: dim,
            math: Style::new().add_modifier(Modifier::ITALIC),
            footnote: Style::new(),
            table_border: dim,
            table_header: bold,
            table_row_rules: true,
            small: dim,
            syntax: None,
            classes: Vec::new(),
        }
    }
}
