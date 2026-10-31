#![allow(clippy::unwrap_used)]
use super::*;

mod structure;

fn kinds(lines: &[Line]) -> Vec<Kind> {
    lines.iter().map(|line| line.kind).collect()
}

#[core::prelude::v1::test]
fn markdown_structure_becomes_typed_lines() {
    let lines = parse("### Added\n\n\n- New `herdr x` flag\nPlain text\n```\nrun --this\n```\n\n");
    assert_eq!(
        kinds(&lines),
        [
            Kind::Heading,
            Kind::Blank,
            Kind::Bullet,
            Kind::Paragraph,
            Kind::Code,
        ]
    );
    assert_eq!(lines[0].text, "Added");
    assert_eq!(lines[2].text, "New herdr x flag");
    assert_eq!(&lines[2].text[lines[2].code[0].clone()], "herdr x");
    assert_eq!(lines[4].text, "run --this");
    assert!(lines[4].links.is_empty() && lines[4].code.is_empty());
}

#[core::prelude::v1::test]
fn only_web_links_become_clickable() {
    let line = &parse(
        "See [docs](https://herdr.dev/a?b=1) or https://example.com/x). \
         [bad](javascript:alert(1)) file:///etc/passwd [x](http://) `https://in.code`",
    )[0];
    let urls: Vec<_> = line.links.iter().map(|link| link.url.as_str()).collect();
    assert_eq!(urls, ["https://herdr.dev/a?b=1", "https://example.com/x"]);
    assert_eq!(&line.text[line.links[0].range.clone()], "docs");
    assert!(line.text.contains("[bad](javascript:alert(1))"));
    assert!(line.text.contains("file:///etc/passwd"));
    assert_eq!(&line.text[line.code[0].clone()], "https://in.code");
}

#[core::prelude::v1::test]
fn highlights_are_in_text_order() {
    let line = &parse("[a](https://a.example) `b` https://c.example **d**")[0];
    let starts: Vec<_> = line
        .highlights()
        .map(|(range, mark)| (range.start, mark))
        .collect();
    assert_eq!(
        starts,
        [
            (0, Mark::Link),
            (2, Mark::Code),
            (4, Mark::Link),
            (22, Mark::Bold)
        ]
    );
    assert_eq!(line.text, "a b https://c.example d");
}

#[core::prelude::v1::test]
fn controls_and_bidi_overrides_are_stripped() {
    let lines = parse("- \u{1b}[31mred\u{7} \u{202e}txt\u{2066}\r\n```\n\u{1b}]52;c;x\u{7}\n```");
    assert_eq!(lines[0].text, "[31mred txt");
    assert_eq!(lines[1].text, "]52;c;x");
}

#[core::prelude::v1::test]
fn bodies_are_bounded() {
    let body = "x\n".repeat(MAX_LINES * 2);
    assert_eq!(parse(&body).len(), MAX_LINES);
    let long = "é".repeat(MAX_BODY);
    let lines = parse(&long);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text.chars().count(), MAX_LINE);
    let url = format!("https://example.com/{}", "a".repeat(MAX_URL));
    assert!(parse(&url)[0].links.is_empty());
}

#[core::prelude::v1::test]
fn prepared_lines_follow_the_body() {
    let prepared = Prepared::default();
    let first = prepared.lines("- one");
    assert!(Rc::ptr_eq(&first, &prepared.lines("- one")));
    assert_eq!(prepared.lines("- two")[0].text, "two");
}

#[core::prelude::v1::test]
fn unmatched_markers_stay_literal() {
    let line = &parse("a `b [c](d e **** **f")[0];
    assert_eq!(line.text, "a `b [c](d e **** **f");
    assert!(line.code.is_empty() && line.bold.is_empty() && line.links.is_empty());
}
