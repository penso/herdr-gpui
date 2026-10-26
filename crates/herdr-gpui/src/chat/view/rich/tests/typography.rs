use super::*;

#[core::prelude::v1::test]
fn prose_is_one_shaped_text_with_unicode_safe_font_and_link_ranges() {
    let content = vec![
        Inline::Text("বাংলা α ".into(), Style::default()),
        Inline::Text(
            "let β".into(),
            Style {
                code: true,
                ..Style::default()
            },
        ),
        Inline::Break,
        Inline::Text(
            "docs".into(),
            Style {
                bold: true,
                link: Some("https://example.org".into()),
                ..Style::default()
            },
        ),
    ];
    let Some(prose) = prose_spans(&content) else {
        panic!("plain text")
    };
    assert_eq!(prose.text, "বাংলা α let β\ndocs");
    let code: Vec<_> = prose
        .spans
        .iter()
        .filter(|(_, style)| style.code)
        .map(|(range, _)| &prose.text[range.clone()])
        .collect();
    assert_eq!(code, ["let β"]);
    let links: Vec<_> = prose
        .spans
        .iter()
        .filter_map(|(range, style)| {
            style
                .link
                .as_deref()
                .map(|url| (&prose.text[range.clone()], url, style.bold))
        })
        .collect();
    assert_eq!(links, [("docs", "https://example.org", true)]);
    assert!(
        prose
            .spans
            .iter()
            .all(|(range, _)| prose.text.is_char_boundary(range.start)
                && prose.text.is_char_boundary(range.end))
    );
}

#[core::prelude::v1::test]
fn mixed_math_retains_inline_atom_layout_and_headings_have_hierarchy() {
    let content = [Inline::Math {
        id: VisualId::of("test"),
        source: "x".into(),
    }];
    assert!(prose_spans(&content).is_none());
    assert!(heading_scale(HeadingLevel::H1) > heading_scale(HeadingLevel::H2));
    assert!(heading_scale(HeadingLevel::H2) > heading_scale(HeadingLevel::H3));
    assert!(heading_scale(HeadingLevel::H3) > heading_scale(HeadingLevel::H4));
}
