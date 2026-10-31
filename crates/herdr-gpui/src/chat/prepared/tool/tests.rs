use super::*;

#[allow(clippy::unwrap_used)]
mod diff;
#[allow(clippy::unwrap_used)]
mod diff_counts;

#[test]
fn explicit_command_and_code_fields_preserve_original_arguments() {
    let raw = r#"{"cmd":"printf 'café\\t🦀'","cwd":"/synthetic/project","description":"Check Unicode output"}"#;
    let (preview, _) = previews(
        &[ToolContent::Text { text: raw.into() }],
        ToolKind::Execute,
        &mut Visuals::default(),
    );
    assert_eq!(preview[0].label, "Check Unicode output");
    assert_eq!(preview[0].code.language, "bash");
    assert_eq!(&*preview[0].code.source, "printf 'café\\t🦀'");
    assert_eq!(&*preview[0].original, raw);
    let raw = r#"{"code":"const value = 4;","language":"javascript"}"#;
    let (_, language, source) = text_source(raw);
    assert_eq!(language, "javascript");
    assert_eq!(&*source, "const value = 4;");
}

#[test]
fn opaque_scripts_and_malformed_arguments_are_not_interpreted() {
    for raw in [
        "const work = chooseAction();\nawait work();",
        "{\"cmd\":",
        "output\nwith ANSI \u{1b}[0m",
    ] {
        let (label, language, source) = text_source(raw);
        assert_eq!(label, "Output / source");
        assert!(language.is_empty());
        assert_eq!(&*source, raw);
    }
    let (_, language, source) = text_source(r#"{"code":"dynamicActions()"}"#);
    assert!(language.is_empty());
    assert_eq!(&*source, "dynamicActions()");
    let (_, language, source) = text_source(r#"{"path":"/synthetic/file.rs"}"#);
    assert_eq!(language, "json");
    assert!(source.contains("path"));
}

#[test]
fn diff_preview_combines_provided_sides_and_preserves_original_copy() {
    let (preview, _) = previews(
        &[ToolContent::Diff {
            path: "src/example.rs".into(),
            old: Some("old\n\tα".into()),
            new: "new\n\t🦀".into(),
        }],
        ToolKind::Edit,
        &mut Visuals::default(),
    );
    assert_eq!(preview[0].code.language, "diff");
    assert!(preview[0].code.source.contains("-old\n-\tα\n"));
    assert!(preview[0].code.source.contains("+new\n+\t🦀\n"));
    assert!(preview[0].before.is_none());
    assert_eq!(
        &*preview[0].original,
        "src/example.rs\nBefore:\nold\n\tα\nAfter:\nnew\n\t🦀"
    );
}

#[test]
fn payload_count_is_bounded_and_overflow_remains_copyable() {
    let content = (0..MAX_PARTS + 2)
        .map(|i| ToolContent::Text {
            text: format!("item {i}"),
        })
        .collect::<Vec<_>>();
    let summary = summarize(&content, ToolKind::Other, &mut Visuals::default());
    assert_eq!(summary.previews.len(), MAX_PARTS);
    assert_eq!(summary.remainder.as_deref(), Some("item 16\n\nitem 17"));
    let raw = format!(
        r#"{{"code":"{}","language":"javascript"}}"#,
        "x".repeat(STRUCTURED_BYTES)
    );
    let (_, language, source) = text_source(&raw);
    assert!(language.is_empty());
    assert_eq!(&*source, raw);
}
