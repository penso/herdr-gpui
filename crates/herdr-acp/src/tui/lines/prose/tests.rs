#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn render(raw: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    prose(&mut lines, raw, width);
    lines
}

fn text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// The style of the span holding `needle` on `line`.
fn style_of(line: &Line<'_>, needle: &str) -> Style {
    line.spans
        .iter()
        .find(|span| span.content.contains(needle))
        .map(|span| span.style)
        .unwrap_or_else(|| panic!("no span holds {needle:?} in {:?}", text(line)))
}

#[test]
fn inline_marks_are_drawn_not_shown() {
    let lines = render("Run `just ci`, then **push** to [the PR](https://x/1).", 80);
    assert_eq!(
        text(&lines[0]),
        "  Run just ci, then push to the PR (https://x/1)."
    );
    assert_eq!(style_of(&lines[0], "just ci").fg, Some(Color::Cyan));
    assert!(
        style_of(&lines[0], "push")
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert!(
        style_of(&lines[0], "the PR")
            .add_modifier
            .contains(Modifier::UNDERLINED)
    );
}

#[test]
fn code_inside_strong_keeps_both() {
    let lines = render("**Send is `off`**", 80);
    assert_eq!(text(&lines[0]), "  Send is off");
    let code = style_of(&lines[0], "off");
    assert_eq!(code.fg, Some(Color::Cyan));
    assert!(code.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn unclosed_marks_stay_as_written() {
    for raw in [
        "a ` stray tick",
        "2 ** 3 is eight",
        "``",
        "see [notes] later",
        "glob *.rs and snake_case_name",
    ] {
        assert_eq!(text(&render(raw, 80)[0]), format!("  {raw}"), "{raw}");
    }
}

#[test]
fn block_marks_become_leads() {
    assert_eq!(text(&render("## Plan", 40)[0]), "  Plan");
    assert!(
        style_of(&render("## Plan", 40)[0], "Plan")
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert_eq!(text(&render("- first", 40)[0]), "  • first");
    assert_eq!(text(&render("  * nested", 40)[0]), "    • nested");
    assert_eq!(text(&render("12. twelfth", 40)[0]), "  12. twelfth");
    assert_eq!(text(&render("> quoted", 40)[0]), "  │ quoted");
    // Not block marks: no space after them.
    assert_eq!(text(&render("#hashtag", 40)[0]), "  #hashtag");
    assert_eq!(text(&render("-1 degrees", 40)[0]), "  -1 degrees");
    assert_eq!(text(&render("3.14 is pi", 40)[0]), "  3.14 is pi");
}

#[test]
fn wrapped_items_hang_under_their_text() {
    let lines = render("- **Typing** switches it back to send", 20);
    let rows: Vec<_> = lines.iter().map(text).collect();
    assert_eq!(rows, ["  • Typing switches", "    it back to send"]);
}

#[test]
fn no_row_is_wider_than_asked() {
    let raw = "A `very_long_identifier_that_cannot_break` and **more words here** to wrap";
    for width in [6, 12, 30] {
        for line in render(raw, width) {
            let used: usize = text(&line).chars().map(|ch| ch.width().unwrap_or(0)).sum();
            assert!(used <= width, "{width}: {:?}", text(&line));
        }
    }
}

#[test]
fn tabs_become_spaces() {
    assert_eq!(text(&render("a\tb", 40)[0]), "  a    b");
}

#[test]
fn a_link_keeps_its_address_readable() {
    let lines = render(
        "See [the PR](https://example.com/123) and [https://a.b](https://a.b).",
        80,
    );
    assert_eq!(
        text(&lines[0]),
        "  See the PR (https://example.com/123) and https://a.b."
    );
    assert_eq!(
        style_of(&lines[0], "(https://example.com/123)").fg,
        Some(Color::DarkGray)
    );
}
