//! Highlights fenced code by its language, with the `syntax` feature.
//!
//!     cargo run --example syntax --features syntax

mod common;

use ratmark::{MarkdownState, Theme};

const MARKDOWN: &str = r#"Fenced code is colored by its language:

```rust
fn main() {
    let name = "world";
    println!("Hello, {name}!"); // A comment.
}
```

```sh
cargo run --example syntax --features syntax
```

An unknown language, or none, stays plain:

```
just text
```
"#;

fn main() -> std::io::Result<()> {
    // Any of syntect's bundled themes. Only their foreground colors are
    // used, over the theme's code block background.
    let theme = Theme {
        syntax: Some("base16-ocean.dark"),
        ..Theme::default()
    };
    common::print(&mut MarkdownState::new(MARKDOWN), &theme)
}
