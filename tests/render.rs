use ratmark::{Region, Rendered, Theme};
use unicode_width::UnicodeWidthStr;

fn lines(markdown: &str, width: u16) -> Vec<String> {
    Rendered::render(markdown, width, &Theme::default())
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

#[test]
fn paragraphs_wrap_with_gaps() {
    assert_eq!(
        lines("one two three\n\nfour", 8),
        ["one two", "three", "", "four"]
    );
}

#[test]
fn headings_have_no_marks_by_default() {
    assert_eq!(lines("## Hi there", 40), ["Hi there"]);
}

#[test]
fn headings_can_show_their_marks() {
    let theme = Theme {
        heading_marker: Some(Default::default()),
        ..Theme::default()
    };
    let rendered = Rendered::render("## Hi there", 40, &theme);
    let text: String = rendered.lines[0]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert_eq!(text, "## Hi there");
    assert_eq!(rendered.headings[0].text, "Hi there");
}

#[cfg(feature = "html")]
#[test]
fn html_headings_show_marks_too() {
    let theme = Theme {
        heading_marker: Some(Default::default()),
        ..Theme::default()
    };
    let text: Vec<String> = Rendered::render("<h3>And here</h3>", 40, &theme)
        .lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert_eq!(text, ["### And here"]);
}

#[cfg(not(feature = "html"))]
#[test]
fn html_is_left_out() {
    assert_eq!(
        lines("<ul><li>a</li></ul>\n\nafter <b>b</b>", 40),
        ["after b"]
    );
}

#[test]
fn nested_lists_indent() {
    let md = "- a\n- b long words\n  - c\n1. d\n2. e";
    assert_eq!(
        lines(md, 10),
        ["• a", "• b long", "  words", "  ◦ c", "", "1. d", "2. e"]
    );
}

#[test]
fn quotes_keep_their_bar_when_wrapped() {
    assert_eq!(lines("> one two three", 9), ["▎ one two", "▎ three"]);
}

#[test]
fn quotes_start_after_a_plain_gap() {
    assert_eq!(lines("a\n\n> b\n>\n> c", 20), ["a", "", "▎ b", "▎", "▎ c"]);
}

#[test]
fn code_blocks_fill_the_width() {
    let rendered = Rendered::render("```\nfn x\n```", 12, &Theme::default());
    let line = &rendered.lines[0];
    assert_eq!(line.width(), 12);
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text.trim_end(), " fn x");
}

#[test]
fn long_code_lines_are_cut_off() {
    let out = lines("```\nabcdefgh\nab\n```", 6);
    assert_eq!(out, [" abcd›", " ab"]);
}

#[test]
fn wide_code_blocks_shift_sideways() {
    let rendered = Rendered::render("> ```\n> abcdefgh\n> ab\n> ```", 8, &Theme::default());
    let block = &rendered.overflows[0];
    assert_eq!((block.lines.clone(), block.x, block.width), (0..2, 2, 4));
    assert_eq!(block.max_shift(), 4);
    let row = |i, shift| block.row(i, shift).to_string();
    assert_eq!(row(0, 2), "‹cdef›");
    assert_eq!(row(0, 9), "‹efgh ");
    assert_eq!(row(1, 2), "‹     ");
}

#[test]
fn links_record_where_they_land() {
    let rendered = Rendered::render("go [over there](/x) now", 10, &Theme::default());
    let link = &rendered.links[0];
    assert_eq!(link.url, "/x");
    assert_eq!(
        link.regions,
        [
            Region {
                line: 0,
                start: 3,
                end: 7
            },
            Region {
                line: 1,
                start: 0,
                end: 5
            }
        ]
    );
    assert_eq!(rendered.link_at(1, 2).map(|l| l.url.as_str()), Some("/x"));
    assert_eq!(rendered.link_at(0, 0), None);
}

#[cfg(feature = "html")]
#[test]
fn html_links_and_lists() {
    let md = "<ul>\n<li><a href=\"/a\">A</a> <small>1 word</small></li>\n<li>B</li>\n</ul>";
    let rendered = Rendered::render(md, 40, &Theme::default());
    assert_eq!(rendered.links[0].url, "/a");
    assert_eq!(lines(md, 40), ["• A 1 word", "• B"]);
}

#[test]
fn tables_line_up() {
    let md = "| a | bb |\n|---|---:|\n| ccc | d |";
    assert_eq!(lines(md, 40), ["a   │ bb", "────┼───", "ccc │  d"]);
}

#[cfg(feature = "kramdown")]
#[test]
fn kramdown_attribute_lists_are_dropped() {
    assert_eq!(
        lines("## Notes\n{:.no_toc}\n\ntext", 40),
        ["Notes", "", "text"]
    );
}

#[test]
fn images_become_links() {
    let rendered = Rendered::render("![a cat](/cat.png)", 40, &Theme::default());
    assert_eq!(rendered.links[0].url, "/cat.png");
    assert_eq!(lines("![a cat](/cat.png)", 40), ["[image: a cat]"]);
}

/// Renders every Markdown file under `$RATMARK_CORPUS` at several widths,
/// checking nothing panics and no line is too wide.
#[test]
fn corpus() {
    let Ok(dir) = std::env::var("RATMARK_CORPUS") else {
        return;
    };
    let mut stack = vec![std::path::PathBuf::from(dir)];
    let mut count = 0;
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            stack.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
            continue;
        }
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let markdown = std::fs::read_to_string(&path).unwrap();
        for width in [1, 8, 20, 45, 80, 200] {
            for (i, line) in lines(&markdown, width).iter().enumerate() {
                assert!(
                    line.width() <= usize::from(width),
                    "{}:{i} is {} wide at {width}: {line:?}",
                    path.display(),
                    line.width(),
                );
            }
        }
        count += 1;
    }
    assert!(count > 0, "no Markdown found");
}

#[test]
fn headings_are_listed() {
    let rendered = Rendered::render("intro\n\n## One *two*\n\ntext", 40, &Theme::default());
    let heading = &rendered.headings[0];
    assert_eq!(
        (heading.level, heading.text.as_str(), heading.line),
        (2, "One two", 2)
    );
    let text: String = rendered.lines[2]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert_eq!(text, "One two");
}

#[cfg(feature = "html")]
#[test]
fn html_items_hang_their_wrapped_lines() {
    let md = "<ul><li>one two three</li><li>a<br>b</li></ul>\n\nafter";
    assert_eq!(
        lines(md, 9),
        ["• one two", "  three", "• a", "  b", "", "after"]
    );
}

#[cfg(feature = "html")]
#[test]
fn html_headings_after_html_lists_get_a_gap() {
    let md = "<ul><li>a</li></ul><h2>B</h2>";
    assert_eq!(lines(md, 9), ["• a", "", "B"]);
}

#[cfg(feature = "html")]
#[test]
fn html_pre_keeps_its_lines() {
    let md = "<pre><code>$ fortune\n<span>a  b</span>\n\n$ cd x</code></pre>\n\nafter";
    let text = lines(md, 40);
    assert_eq!(
        text.iter().map(|l| l.trim_end()).collect::<Vec<_>>(),
        [" $ fortune", " a  b", "", " $ cd x", "", "after"]
    );
}

/// The background behind `text`, wherever it's drawn first.
#[cfg(any(feature = "html", feature = "kramdown"))]
fn background(rendered: &Rendered, text: &str) -> Option<ratatui_core::style::Color> {
    rendered
        .lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .find(|s| s.content.contains(text))
        .and_then(|s| s.style.bg)
}

#[cfg(any(feature = "html", feature = "kramdown"))]
fn class_theme() -> Theme {
    use ratatui_core::style::{Color, Style};
    Theme {
        classes: vec![
            (".hl".into(), Style::new().bg(Color::Red)),
            (".hl.other".into(), Style::new().bg(Color::Blue)),
        ],
        ..Theme::default()
    }
}

#[cfg(all(feature = "html", feature = "kramdown"))]
#[test]
fn classes_color_code_blocks() {
    use ratatui_core::style::Color;
    let markdown = "<div class=\"hl\" markdown=\"1\">\n```\nfn x\n```\n</div>\n\n<div class=\"hl other\" markdown=\"1\">\n```\nfn y\n```\n</div>\n\n```\nfn z\n```\n";
    let rendered = Rendered::render(markdown, 20, &class_theme());
    assert_eq!(background(&rendered, "fn x"), Some(Color::Red));
    assert_eq!(background(&rendered, "fn y"), Some(Color::Blue));
    assert_eq!(
        background(&rendered, "fn z"),
        Theme::default().code_block.bg
    );
}

#[cfg(feature = "html")]
#[test]
fn classes_last_until_their_element_closes() {
    use ratatui_core::style::Color;
    let markdown = "<div class=\"hl\"><div>inner</div> still</div> after";
    let rendered = Rendered::render(markdown, 40, &class_theme());
    assert_eq!(background(&rendered, "inner"), Some(Color::Red));
    assert_eq!(background(&rendered, "still"), Some(Color::Red));
    assert_eq!(background(&rendered, "after"), Theme::default().text.bg);
}

#[cfg(feature = "kramdown")]
#[test]
fn kramdown_classes_style_inline_code() {
    use ratatui_core::style::Color;
    let rendered = Rendered::render("`x`{:.hl} and `y`", 40, &class_theme());
    assert_eq!(background(&rendered, "x"), Some(Color::Red));
    assert_ne!(background(&rendered, "y"), Some(Color::Red));
}
