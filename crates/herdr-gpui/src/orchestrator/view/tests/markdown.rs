use crate::orchestrator::view::markdown::{Markdown, without_html};
use crate::release_notes::Kind;

#[test]
fn github_html_without_text_is_dropped_and_media_named() {
    let body = "<!-- Please fill in -->\n### What happened?\nIt broke.<br>See <img width=\"750\" src=\"https://x/y.png\">\n<details><summary>Logs</summary>trace</details>";
    assert_eq!(
        without_html(body),
        "\n### What happened?\nIt broke.\nSee [image]\nLogstrace"
    );
    // A < that opens no known tag is text, as in `a < b` or `Vec<T>`.
    assert_eq!(
        without_html("a < b and Vec<T> <custom>"),
        "a < b and Vec<T> <custom>"
    );
    // An unclosed comment hides the rest, as GitHub does.
    assert_eq!(without_html("kept <!-- open"), "kept ");
}

#[test]
fn bodies_become_markdown_lines_once_per_change() {
    let markdown = Markdown::default();
    let first = markdown.lines("### Steps\n- **bold** step\n");
    let kinds: Vec<_> = first.iter().map(|line| line.kind).collect();
    assert_eq!(kinds, [Kind::Heading, Kind::Bullet]);
    assert_eq!(first[1].bold.len(), 1, "bold spans are kept");
    let again = markdown.lines("### Steps\n- **bold** step\n");
    assert!(
        std::rc::Rc::ptr_eq(&first, &again),
        "the same body is parsed once"
    );
    let changed = markdown.lines("Other");
    assert!(!std::rc::Rc::ptr_eq(&first, &changed));
}

#[test]
fn code_keeps_its_html_and_prose_decodes_entities() {
    let body = "Use `<div>` here.\n```html\n<div>example</div>\n<!-- note -->\n```\nAfter <b>bold</b>&nbsp;&amp;&nbsp;`&amp;`";
    assert_eq!(
        without_html(body),
        "Use `<div>` here.\n```html\n<div>example</div>\n<!-- note -->\n```\nAfter bold & `&amp;`"
    );
    // An entity for a tag stays the character, not a tag to strip.
    assert_eq!(without_html("&lt;b&gt; &amp;lt;"), "<b> &lt;");
    // An unclosed span is a lone backtick.
    assert_eq!(without_html("a ` b <b>c</b>"), "a ` b c");
}
