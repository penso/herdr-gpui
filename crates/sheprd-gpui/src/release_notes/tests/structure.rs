#![allow(clippy::unwrap_used)]

use super::*;

fn kinds(lines: &[Line]) -> Vec<Kind> {
    lines.iter().map(|line| line.kind).collect()
}

#[core::prelude::v1::test]
fn lists_quotes_rules_and_tables_are_typed() {
    let lines = parse(
        "1. first\n2) second\n  - nested\n    + deeper\n> quoted *text*\n---\n\
         | Name | Size |\n|:-----|-----:|\n| a.rs | 12 |\n",
    );
    assert_eq!(
        kinds(&lines),
        [
            Kind::Numbered(1),
            Kind::Numbered(2),
            Kind::Bullet,
            Kind::Bullet,
            Kind::Quote,
            Kind::Rule,
            Kind::Table,
            Kind::Table,
        ]
    );
    assert_eq!(lines[0].text, "first");
    let indents: Vec<_> = lines[..4].iter().map(|line| line.indent).collect();
    assert_eq!(indents, [0, 0, 1, 2]);
    assert_eq!(lines[4].text, "quoted text");
    assert_eq!(lines[6].text, "Name \u{2502} Size");
    assert_eq!(lines[7].text, "a.rs \u{2502} 12");
}

#[core::prelude::v1::test]
fn emphasis_needs_a_word_boundary() {
    let line = &parse("an *emphasized* and _also_ word, but snake_case_name and 2*3*4 stay")[0];
    let spans: Vec<_> = line
        .italic
        .iter()
        .map(|range| &line.text[range.clone()])
        .collect();
    assert_eq!(spans, ["emphasized", "also"]);
    assert!(line.text.contains("snake_case_name"));
    assert!(line.text.contains("2*3*4"));
    let line = &parse("**bold** then *it*")[0];
    assert_eq!(&line.text[line.bold[0].clone()], "bold");
    assert_eq!(&line.text[line.italic[0].clone()], "it");
}

#[core::prelude::v1::test]
fn numbers_need_a_marker_and_a_space() {
    assert_eq!(kinds(&parse("2024.10 release")), [Kind::Paragraph]);
    assert_eq!(kinds(&parse("1234567890. too long")), [Kind::Paragraph]);
    assert_eq!(kinds(&parse("- - -")), [Kind::Rule]);
}
