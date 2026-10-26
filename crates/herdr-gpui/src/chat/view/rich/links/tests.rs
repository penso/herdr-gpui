#![allow(clippy::unwrap_used)]
use super::*;

#[core::prelude::v1::test]
fn a_remote_styled_link_shows_its_path_once_and_keeps_web_links() {
    let blocks = crate::chat::rich::parse(
        "[Changes **and** `details`](/tmp/file.md) and [web](https://example.org).",
        &mut Visuals::default(),
    )
    .unwrap();
    let Block::Paragraph { content, .. } = &blocks[0] else {
        panic!("paragraph")
    };
    let shown = remote_content(content);
    let visible: String = shown
        .iter()
        .filter_map(|part| match part {
            Inline::Text(text, _) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(visible, "Changes and details (/tmp/file.md) and web.");
    assert!(
        shown
            .iter()
            .any(|part| matches!(part, Inline::Text(text, style)
        if text == "and" && style.bold && style.link.is_none()))
    );
    assert!(
        shown
            .iter()
            .any(|part| matches!(part, Inline::Text(text, style)
        if text == "details" && style.code && style.link.is_none()))
    );
    let web = shown.iter().find_map(|part| match part {
        Inline::Text(text, style) if text == "web" => style.link.as_deref(),
        _ => None,
    });
    assert_eq!(
        web,
        Some("https://example.org/"),
        "the existing URL parser canonicalizes a host-only address"
    );
    assert!(
        content
            .iter()
            .any(|part| matches!(part, Inline::Text(_, style)
        if style.link.as_deref() == Some("/tmp/file.md"))),
        "local source remains unchanged"
    );
}

#[core::prelude::v1::test]
fn separated_remote_links_each_keep_their_destination() {
    let blocks = crate::chat::rich::parse(
        "[First](/tmp/file.md) then [second](/tmp/file.md)",
        &mut Visuals::default(),
    )
    .unwrap();
    let Block::Paragraph { content, .. } = &blocks[0] else {
        panic!("paragraph")
    };
    let visible: String = remote_content(content)
        .iter()
        .filter_map(|part| match part {
            Inline::Text(text, _) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(visible, "First (/tmp/file.md) then second (/tmp/file.md)");
}
