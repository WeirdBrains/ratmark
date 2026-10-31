# ratmark

Render Markdown into [Ratatui] text.

```rust
let theme = ratmark::Theme::default();
let mut state = ratmark::MarkdownState::new(markdown);

// Each frame:
frame.render_stateful_widget(ratmark::MarkdownView::new(&theme), area, &mut state);

// On input:
state.scroll_by(1);
state.focus_next();
if let Some(link) = state.focused() { open(&link.url) }
if let Some(link) = state.link_at(mouse.column, mouse.row) { open(&link.url) }
```

`MarkdownView` lays the document out to fit, again whenever the width
changes, and draws it scrolled with the focused link highlighted. Ratatui
cells can't carry a URL, so the state keeps track of where every link landed,
which is what makes keyboard focus and clicks work on links in the middle of
wrapped text. For drawing a document some other way, `Rendered::render`
returns the wrapped lines and link positions directly.

With the `native-links` feature, the view also writes each link's URL into
its cells, so backends that support it draw links the terminal or browser
knows about, like OSC 8. It needs Ratatui's cell extensions, which aren't
released yet ([ratatui#2854]), and `MarkdownState::with_base` makes the links
absolute.

It handles CommonMark plus tables, footnotes, strikethrough, task lists and
math. Code blocks can be syntax highlighted with [syntect], through the
`syntax` feature, which is off by default. It uses syntect's pure Rust regex
engine, so ratmark also builds for `wasm32-unknown-unknown` and works in the
browser with something like [Ratzilla].

Markdown written for real sites tends to have HTML mixed in. With the `html`
feature, off by default, ratmark reads the text, links and simple formatting
out of it with [html5gum]'s tokenizer instead of printing tags. Without it,
HTML is left out.

A few optional features help with Markdown from static sites.
`front-matter-yaml` and `front-matter-toml` split YAML front matter, between
`---` lines, or TOML front matter, between `+++` lines, off the top of a
document, and deserialize it into your own serde types with [serde-saphyr]
or [toml].

The `kramdown` feature understands a couple of kramdown habits, since
that's what Jekyll uses. Attribute lists like `{:.no_toc}` are dropped, and
Markdown inside `markdown="1"` elements is rendered as Markdown.

Two themes come with it. `Theme::gruvbox()` is the default, and
`Theme::plain()` uses no colors so the terminal's own palette shows through.
Every style is a public field, so making your own is a struct literal.
`Theme::classes` styles elements by class, written like CSS selectors. It
reads classes from HTML, like a `<div class="note">` around a code block,
which then takes the class's background, and from kramdown attribute lists
after inline code, like `` `x`{:.note} ``.

## Credit

The idea and the look come from Tyler Bloom's blog, [The Avid
Rustacean][avid-rustacean], which renders its posts with Ratatui in the
browser. ratmark is a fresh implementation, not a copy of that code.

## Testing

`cargo test` runs the unit tests. To check a pile of real documents, point
`RATMARK_CORPUS` at a directory of Markdown files. Every file is rendered at
several widths, checking that nothing panics and no line is too wide.

```sh
RATMARK_CORPUS=~/site/_site cargo test --features kramdown corpus
```

`cargo run --example dump -- FILE WIDTH` prints a file as rendered, without
colors, which is handy for eyeballing layout.

## Examples

Each feature has an example that prints a short document, drawn by
`MarkdownView` with Ratatui's crossterm backend, to the terminal in color.
They need an interactive terminal, not a pipe:

```sh
cargo run --example syntax --features syntax
cargo run --example front_matter --features front-matter-yaml
cargo run --example kramdown --features kramdown
cargo run --example html --features html
cargo run --example native_links --features native-links
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

[Ratatui]: https://ratatui.rs
[syntect]: https://github.com/trishume/syntect
[html5gum]: https://github.com/untitaker/html5gum
[serde-saphyr]: https://github.com/bourumir-wyngs/serde-saphyr
[toml]: https://github.com/toml-rs/toml
[Ratzilla]: https://github.com/orhun/ratzilla
[avid-rustacean]: https://github.com/TylerBloom/avid-rustacean
[ratatui#2854]: https://github.com/ratatui/ratatui/issues/2854
