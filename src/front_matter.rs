//! A preamble of metadata at the very top of a file, fenced off from the
//! document after it. Static site generators like Jekyll and Hugo read it
//! from Markdown for a page's title, date, tags and layout, and it isn't
//! meant to be shown as part of the page.
//!
//! Each language it can be written in has its own feature:
//!
//! - `front-matter-yaml`: YAML between `---` lines
//! - `front-matter-toml`: TOML between `+++` lines
//!
//! Nothing else in ratmark looks for front matter. It's a prepass:
//! [`FrontMatter::split`] it off, give the body to
//! [`MarkdownState`](crate::MarkdownState) or
//! [`Rendered::render`](crate::Rendered::render), and
//! [`FrontMatter::deserialize`] the front matter into your own serde type.
//!
//! ```
//! # #[cfg(feature = "front-matter-yaml")] {
//! use ratmark::front_matter::FrontMatter;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Meta {
//!     title: String,
//!     tags: Vec<String>,
//! }
//!
//! let (front, body) = FrontMatter::split("---\ntitle: 'Hi: there'\ntags: [a, b]\n---\n\n# Hi");
//! let meta: Meta = front.unwrap().deserialize().unwrap();
//! assert_eq!(meta.title, "Hi: there");
//! assert_eq!(meta.tags, ["a", "b"]);
//! assert_eq!(body, "\n# Hi");
//! # }
//! ```

use std::fmt;

use serde::Deserialize;

/// YAML front matter fenced by `---` lines, or TOML front matter fenced by
/// `+++` lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontMatter<'a> {
    /// Which language the front matter is in, by its fences.
    pub format: Format,
    /// The text between the fences, unparsed.
    pub raw: &'a str,
}

/// The language front matter is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// YAML, between `---` lines, with the `front-matter-yaml` feature.
    #[cfg(feature = "front-matter-yaml")]
    Yaml,
    /// TOML, between `+++` lines, with the `front-matter-toml` feature.
    #[cfg(feature = "front-matter-toml")]
    Toml,
}

impl Format {
    const ALL: &[Format] = &[
        #[cfg(feature = "front-matter-yaml")]
        Format::Yaml,
        #[cfg(feature = "front-matter-toml")]
        Format::Toml,
    ];

    /// The line above and below the front matter.
    fn fence(self) -> &'static str {
        match self {
            #[cfg(feature = "front-matter-yaml")]
            Format::Yaml => "---",
            #[cfg(feature = "front-matter-toml")]
            Format::Toml => "+++",
        }
    }
}

impl<'a> FrontMatter<'a> {
    /// Splits front matter off the start of `text`, returning it and the
    /// rest of the document. Front matter opens with a fence on the first
    /// line, and closes at the next line with the same fence. Either fence
    /// may have trailing whitespace, including the `\r` of a `\r\n` line
    /// ending. Without both, this returns `None` and all of `text`.
    ///
    /// This only finds the fences. Nothing is parsed until
    /// [`FrontMatter::deserialize`].
    pub fn split(text: &'a str) -> (Option<Self>, &'a str) {
        for &format in Format::ALL {
            let is_fence = |line: &&str| line.trim_end() == format.fence();
            let mut lines = text.split_inclusive('\n');
            let Some(open) = lines.next().filter(is_fence) else {
                continue;
            };
            let Some(close) = lines
                .find(is_fence)
                .and_then(|line| text.substr_range(line))
            else {
                continue;
            };
            let front = Self {
                format,
                raw: &text[open.len()..close.start],
            };
            return (Some(front), &text[close.end..]);
        }
        (None, text)
    }

    /// Parses the front matter as its [`Format`] into `T`, with serde-saphyr
    /// for YAML and toml for TOML. `T` may borrow from the text it was split
    /// from. Empty front matter deserializes like an empty map.
    ///
    /// # Errors
    ///
    /// When the front matter isn't valid YAML or TOML, or doesn't fit `T`.
    pub fn deserialize<T: Deserialize<'a>>(&self) -> Result<T, Error> {
        match self.format {
            #[cfg(feature = "front-matter-yaml")]
            Format::Yaml => serde_saphyr::from_str(self.raw).map_err(Error::new),
            #[cfg(feature = "front-matter-toml")]
            Format::Toml => toml::from_str(self.raw).map_err(Error::new),
        }
    }
}

/// Why [`FrontMatter::deserialize`] failed: the front matter isn't valid
/// YAML or TOML, or doesn't fit the type asked for.
#[derive(Debug)]
pub struct Error(Box<dyn std::error::Error + Send + Sync>);

impl Error {
    fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Box::new(error))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[cfg(feature = "front-matter-yaml")]
    #[test]
    fn yaml() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct Meta {
            title: String,
            draft: bool,
            tags: Vec<String>,
            author: Author,
        }
        #[derive(Debug, PartialEq, Deserialize)]
        struct Author {
            name: String,
        }

        let (front, body) = FrontMatter::split(
            "---\ntitle: \"Say \\\"hi\\\"\"\ndraft: false\ntags:\n  - a\n  - b\n\
             author:\n  name: Me\n---\nbody",
        );
        let front = front.unwrap();
        assert_eq!(front.format, Format::Yaml);
        assert_eq!(
            front.deserialize::<Meta>().unwrap(),
            Meta {
                title: "Say \"hi\"".to_owned(),
                draft: false,
                tags: vec!["a".to_owned(), "b".to_owned()],
                author: Author {
                    name: "Me".to_owned()
                },
            }
        );
        assert_eq!(body, "body");
    }

    #[cfg(feature = "front-matter-toml")]
    #[test]
    fn toml() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct Meta {
            title: String,
            weight: u32,
            tags: Vec<String>,
            extra: Extra,
        }
        #[derive(Debug, PartialEq, Deserialize)]
        struct Extra {
            key: String,
        }

        let (front, body) = FrontMatter::split(
            "+++\ntitle = 'Hi'\nweight = 3\ntags = ['a']\n[extra]\nkey = 'v'\n+++\n",
        );
        let front = front.unwrap();
        assert_eq!(front.format, Format::Toml);
        assert_eq!(
            front.deserialize::<Meta>().unwrap(),
            Meta {
                title: "Hi".to_owned(),
                weight: 3,
                tags: vec!["a".to_owned()],
                extra: Extra {
                    key: "v".to_owned()
                },
            }
        );
        assert_eq!(body, "");
    }

    #[cfg(feature = "front-matter-yaml")]
    #[test]
    fn invalid_is_split_off_but_fails_to_deserialize() {
        let (front, body) = FrontMatter::split("---\ntitle: [\n---\nbody");
        let front = front.unwrap();
        assert_eq!(front.raw, "title: [\n");
        assert!(front.deserialize::<HashMap<String, String>>().is_err());
        assert_eq!(body, "body");

        let (front, _) = FrontMatter::split("---\ntitle: [a]\n---\n");
        assert!(
            front
                .unwrap()
                .deserialize::<HashMap<String, String>>()
                .is_err()
        );
    }

    #[test]
    fn empty_front_matter_deserializes_like_an_empty_map() {
        fn fields(text: &str) -> HashMap<String, String> {
            FrontMatter::split(text).0.unwrap().deserialize().unwrap()
        }

        #[cfg(feature = "front-matter-yaml")]
        assert_eq!(fields("---\n---\nbody"), HashMap::new());
        #[cfg(feature = "front-matter-toml")]
        assert_eq!(fields("+++\n+++\nbody"), HashMap::new());
    }

    #[test]
    fn none() {
        for text in [
            "# Hi",
            "--- not front matter",
            "---\nnever closed",
            "+++\nnever closed",
            "---",
            "----\na: 1\n----\n",
            "\n---\na: 1\n---\n",
        ] {
            assert_eq!(FrontMatter::split(text), (None, text));
        }
    }

    #[cfg(feature = "front-matter-yaml")]
    #[test]
    fn fences_may_have_trailing_whitespace() {
        let front = |raw| {
            Some(FrontMatter {
                format: Format::Yaml,
                raw,
            })
        };
        assert_eq!(
            FrontMatter::split("---  \na: 1\n---\t\nbody"),
            (front("a: 1\n"), "body")
        );
        assert_eq!(
            FrontMatter::split("---\r\na: 1\r\n---\r\nbody\r\n"),
            (front("a: 1\r\n"), "body\r\n")
        );
    }

    #[cfg(feature = "front-matter-yaml")]
    #[test]
    fn the_first_closing_fence_ends_it() {
        let front = |raw| {
            Some(FrontMatter {
                format: Format::Yaml,
                raw,
            })
        };
        assert_eq!(
            FrontMatter::split("---\na: 1\n---\nb\n---\n"),
            (front("a: 1\n"), "b\n---\n")
        );
        assert_eq!(FrontMatter::split("---\na: 1\n---"), (front("a: 1\n"), ""));
    }

    #[test]
    fn deserialized_values_borrow_from_the_text() {
        #[derive(Deserialize)]
        struct Meta<'a> {
            title: &'a str,
        }

        /// The title, borrowed from `text` past the [`FrontMatter`].
        fn title(text: &str) -> &str {
            let meta: Meta = FrontMatter::split(text).0.unwrap().deserialize().unwrap();
            meta.title
        }

        #[cfg(feature = "front-matter-yaml")]
        assert_eq!(title("---\ntitle: Hi\n---\n"), "Hi");
        #[cfg(feature = "front-matter-toml")]
        assert_eq!(title("+++\ntitle = 'Hi'\n+++\n"), "Hi");
    }

    #[cfg(all(feature = "front-matter-yaml", feature = "front-matter-toml"))]
    #[test]
    fn fences_must_match() {
        let front = |format, raw| Some(FrontMatter { format, raw });
        assert_eq!(
            FrontMatter::split("---\n+++\nbody"),
            (None, "---\n+++\nbody")
        );
        assert_eq!(
            FrontMatter::split("+++\n---\nbody"),
            (None, "+++\n---\nbody")
        );
        assert_eq!(
            FrontMatter::split("---\n+++\n---\nbody"),
            (front(Format::Yaml, "+++\n"), "body")
        );
        assert_eq!(
            FrontMatter::split("+++\na = 1\n---\n+++\nbody"),
            (front(Format::Toml, "a = 1\n---\n"), "body")
        );
    }
}
