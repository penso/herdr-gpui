use super::*;

#[allow(clippy::unwrap_used)]
mod diff;
#[allow(clippy::unwrap_used)]
mod diff_counts;
#[allow(clippy::unwrap_used)]
mod highlighting;

fn previews(
    content: &[ToolContent],
    kind: ToolKind,
    visuals: &mut Visuals,
) -> (Vec<Preview>, Vec<DiffSummary>) {
    let summary = summarize(content, kind, visuals);
    (summary.previews, summary.diffs)
}

fn text(text: &str) -> ToolContent {
    ToolContent::Text { text: text.into() }
}

fn counts(changes: &[DiffSummary]) -> Vec<(&str, usize, usize)> {
    changes
        .iter()
        .map(|change| (change.path.as_str(), change.added, change.removed))
        .collect()
}

#[test]
fn explicit_command_and_code_fields_preserve_original_arguments() {
    let raw = r#"{"cmd":"printf 'café\\t🦀'","cwd":"/synthetic/project","description":"Check Unicode output"}"#;
    let (preview, _) = previews(&[text(raw)], ToolKind::Execute, &mut Visuals::default());
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
        assert_eq!(label, "Output");
        assert!(language.is_empty());
        assert_eq!(&*source, raw);
    }
    let (_, language, source) = text_source(r#"{"code":"dynamicActions()"}"#);
    assert!(language.is_empty());
    assert_eq!(&*source, "dynamicActions()");
}

#[test]
fn unknown_arguments_stay_readable_json() {
    let raw = r#"{"path":"src/a.rs","limit":3}"#;
    let (preview, _) = previews(&[text(raw)], ToolKind::Other, &mut Visuals::default());
    assert_eq!(preview[0].label, "Arguments");
    assert_eq!(preview[0].code.language, "json");
    assert_eq!(&*preview[0].code.source, raw);
}

#[test]
fn folded_lines_read_arguments_as_what_they_ask_for() {
    let summary = summarize(
        &[text(r#"{"command":"cargo test","timeout":5}"#)],
        ToolKind::Execute,
        &mut Visuals::default(),
    );
    assert_eq!(summary.lines, ["cargo test"]);
}

#[test]
fn a_whole_file_write_shows_its_text_and_invents_no_deletions() {
    let raw = serde_json::json!({"file_path": "src/a.rs", "content": "fn a() {}\nfn b() {}\n"});
    let (preview, changes) = previews(
        &[text(&raw.to_string())],
        ToolKind::Edit,
        &mut Visuals::default(),
    );
    assert_eq!(&*preview[0].code.source, "fn a() {}\nfn b() {}\n");
    assert_eq!(preview[0].code.language, "rs");
    assert!(preview[0].before.is_none());
    assert_eq!(counts(&changes), [("src/a.rs", 2, 0)]);
}

#[test]
fn only_an_edits_patch_counts_as_changed_files() {
    let patch = "src/a.rs\n@@ -1,2 +1,2 @@\n same\n-old\n+new\n";
    let (preview, changes) = previews(&[text(patch)], ToolKind::Edit, &mut Visuals::default());
    assert_eq!(preview[0].code.language, "diff");
    assert_eq!(counts(&changes), [("src/a.rs", 1, 1)]);

    let (preview, changes) = previews(&[text(patch)], ToolKind::Execute, &mut Visuals::default());
    assert_eq!(preview[0].code.language, "diff");
    assert!(changes.is_empty());
}

#[allow(clippy::unwrap_used)]
#[test]
fn fragments_without_a_final_newline_are_not_marked_as_a_files_end() {
    let shown = super::diff::between("a.rs", "let a = 1;", "let a = 2;").unwrap();
    assert!(!shown.source.contains("No newline"));
    assert!(shown.source.ends_with("-let a = 1;\n+let a = 2;\n"));
    let shown = super::diff::between("a.rs", "same", "same\n").unwrap();
    assert!(shown.source.contains("No newline"));
}

#[test]
fn a_patch_past_the_row_limit_reports_no_partial_count() {
    let lines = 600;
    let body = "+line\n".repeat(lines);
    let patch = format!("--- a/b.rs\n+++ b/b.rs\n@@ -0,0 +1,{lines} @@\n{body}");
    assert!(patch_changes(&patch).is_empty());
}

#[test]
fn every_content_item_stays_shown_past_the_structured_limit() {
    let mut content = vec![text("output"); MAX_PARTS];
    content.push(ToolContent::Diff {
        path: "overflow.rs".into(),
        old: Some("old\n".into()),
        new: "new\n".into(),
    });
    content.push(text(r#"{"command":"ls"}"#));
    let (preview, changes) = previews(&content, ToolKind::Edit, &mut Visuals::default());
    assert_eq!(preview.len(), content.len());
    assert!(changes.is_empty(), "it was shown but never compared");
    assert!(preview[MAX_PARTS].before.is_some());
    assert_eq!(&*preview[MAX_PARTS + 1].code.source, r#"{"command":"ls"}"#);
}
