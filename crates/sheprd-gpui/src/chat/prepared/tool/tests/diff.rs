use super::*;
use serde_json::json;

fn text_preview(text: &str, kind: ToolKind) -> Preview {
    previews(
        &[ToolContent::Text { text: text.into() }],
        kind,
        &mut Visuals::default(),
    )
    .0
    .remove(0)
}

#[test]
fn supplied_fragments_keep_context_without_claiming_absolute_line_numbers() {
    let prefix = (0..80)
        .map(|i| format!("unchanged {i}\n"))
        .collect::<String>();
    let old = format!("{prefix}old α\nlast\n");
    let new = format!("{prefix}new 🦀\nlast\n");
    let shown = super::super::diff::between("src/a.rs", &old, &new).unwrap();
    assert_eq!(shown.language, "diff");
    assert!(shown.label.contains("provided text"));
    assert!(
        shown
            .source
            .starts_with("--- src/a.rs\n+++ src/a.rs\n@@ Provided text @@\n")
    );
    assert!(!shown.source.contains("unchanged 70"));
    let first = shown.source.lines().take(10).collect::<Vec<_>>();
    assert!(first.contains(&"-old α"));
    assert!(first.contains(&"+new 🦀"));
    assert!(shown.source.ends_with(" last\n"));
}

#[test]
fn distant_changes_have_separate_fragment_context_and_reorders_are_not_erased() {
    let middle = (0..12).map(|i| format!("same {i}\n")).collect::<String>();
    let shown = super::super::diff::between(
        "a",
        &format!("old\n{middle}tail\n"),
        &format!("new\n{middle}end\n"),
    )
    .unwrap();
    assert_eq!(shown.source.matches("@@ Provided text @@").count(), 2);
    assert!(!shown.source.contains("same 6"));
    assert!(shown.source.contains("-tail\n+end\n"));
    let moved = super::super::diff::between("a", "one\ntwo\n", "two\none\n").unwrap();
    assert!(
        moved
            .source
            .lines()
            .any(|line| line.starts_with('-') && !line.starts_with("---"))
    );
    assert!(
        moved
            .source
            .lines()
            .any(|line| line.starts_with('+') && !line.starts_with("+++"))
    );
}

#[test]
fn final_newline_changes_and_identical_fragments_are_explicit() {
    let shown = super::super::diff::between("a", "same", "same\n").unwrap();
    assert!(
        shown
            .source
            .contains("-same\n\\ No newline at end of supplied text\n+same\n")
    );
    let same = super::super::diff::between("a", "α\n", "α\n").unwrap();
    assert!(same.source.contains("No changes in the supplied text."));
    assert!(!same.source.contains("+α") && !same.source.contains("-α"));
}

#[test]
fn larger_middle_uses_bounded_coarse_replacement_and_oversize_stays_raw() {
    let old = (0..300)
        .map(|i| format!("before {i}\n"))
        .collect::<String>();
    let new = (0..300).map(|i| format!("after {i}\n")).collect::<String>();
    let shown = super::super::diff::between("a", &old, &new).unwrap();
    assert_eq!(
        shown
            .source
            .lines()
            .filter(|line| line.starts_with("-before "))
            .count(),
        300
    );
    assert_eq!(
        shown
            .source
            .lines()
            .filter(|line| line.starts_with("+after "))
            .count(),
        300
    );
    assert!(super::super::diff::between("a", &"x".repeat(STRUCTURED_BYTES), "y").is_none());
    assert!(super::super::diff::between("a", &"x\n".repeat(2_049), "y").is_none());
    let raw = json!({"file_path":"a", "old_string":"x".repeat(STRUCTURED_BYTES), "new_string":"y"})
        .to_string();
    let preview = text_preview(&raw, ToolKind::Edit);
    assert_ne!(preview.code.language, "diff");
    assert_eq!(&*preview.original, raw);
    assert_eq!(&*preview.code.source, raw);
}

#[test]
fn exact_claude_and_pi_edit_inputs_keep_original_json_and_fragment_semantics() {
    for input in [
        json!({"file_path":"src/a.rs", "old_string":"a\n", "new_string":"b\n", "replace_all":false}),
        json!({"file_path":"src/a.rs", "edits":[{"old_string":"a\n", "new_string":"b\n"}]}),
        json!({"path":"src/a.rs", "edits":[{"oldText":"a\n", "newText":"b\n"}]}),
    ] {
        let raw = input.to_string();
        let preview = text_preview(&raw, ToolKind::Edit);
        assert_eq!(preview.code.language, "diff");
        assert!(preview.code.source.contains("-a\n+b\n"));
        assert!(preview.before.is_none());
        assert_eq!(&*preview.original, raw);
    }
    let raw = json!({"file_path":"a", "edits":[
        {"old_string":"α", "new_string":"β"},
        {"old_string":"x", "new_string":"y"}
    ]})
    .to_string();
    let preview = text_preview(&raw, ToolKind::Edit);
    assert_eq!(
        preview.code.source.matches("@@ Provided text @@").count(),
        2
    );
    assert_eq!(preview.code.source.matches("--- a\n+++ a\n").count(), 1);
    assert_eq!(&*preview.original, raw);
}

#[test]
fn argument_interpretation_requires_edit_kind_first_payload_and_typed_unambiguous_fields() {
    let raw = json!({"file_path":"a", "old_string":"before", "new_string":"after"}).to_string();
    assert_eq!(text_preview(&raw, ToolKind::Other).code.language, "json");
    let (later, _) = previews(
        &[
            ToolContent::Text {
                text: "input".into(),
            },
            ToolContent::Text { text: raw.clone() },
        ],
        ToolKind::Edit,
        &mut Visuals::default(),
    );
    assert_eq!(later[1].code.language, "json");
    for input in [
        json!({"file_path":"a", "new_string":"new"}),
        json!({"file_path":"a", "old_string":null, "new_string":"new"}),
        json!({"file_path":"a", "path":"b", "old_string":"old", "new_string":"new"}),
        json!({"file_path":"a\n+++ forged", "old_string":"old", "new_string":"new"}),
        json!({"file_path":"a", "old_string":"old", "new_string":"new", "code":"execute()"}),
        json!({"file_path":"a", "edits":[]}),
        json!({"file_path":"a", "edits":[{"path":"b", "old_string":"x", "new_string":"y"}]}),
        json!({"file_path":"a", "edits":[{"file_path":null, "old_string":"x", "new_string":"y"}]}),
        json!({"file_path":"a", "edits":vec![json!({"old_string":"a", "new_string":"b"}); MAX_PARTS+1]}),
    ] {
        let raw = input.to_string();
        let preview = text_preview(&raw, ToolKind::Edit);
        assert_ne!(preview.code.language, "diff");
        assert_eq!(&*preview.original, raw);
    }
    let script = "await tools.apply_patch('*** Begin Patch\\n*** Delete File: a\\n*** End Patch');";
    assert_ne!(
        text_preview(script, ToolKind::Execute).code.language,
        "diff"
    );
}

#[test]
fn unknown_previous_content_is_code_without_fabricated_additions() {
    let content = [ToolContent::Diff {
        path: "a.rs".into(),
        old: None,
        new: "let α = 1;\n".into(),
    }];
    let summary = summarize(&content, ToolKind::Edit, &mut Visuals::default());
    assert!(summary.diffs.is_empty());
    let preview = &summary.previews[0];
    assert_eq!(preview.code.language, "rs");
    assert_eq!(&*preview.code.source, "let α = 1;\n");
    assert!(preview.label.contains("previous content unavailable"));
    assert!(!preview.original.contains("New file"));
    assert!(preview.original.ends_with("let α = 1;\n"));
}

#[test]
fn valid_unified_patches_keep_hunk_numbers_paths_and_original_bytes() {
    let patches = [
        "--- a/file\n+++ b/file\n@@ -20,2 +20,2 @@ example\n same\n-old\n+new\n",
        "file\n@@ -1 +1 @@\n-old\n+new\n",
        "file\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n",
        "diff --git a/file b/file\nnew file mode 100644\nindex 000..111\n--- /dev/null\n+++ b/file\n@@ -0,0 +1 @@\n+new\n",
        "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n",
        "--- a/file\n+++ b/file\n@@ -1 +1 @@\n---actual removed text\n+++actual added text\n",
    ];
    for raw in patches {
        for kind in [ToolKind::Edit, ToolKind::Execute, ToolKind::Read] {
            let preview = text_preview(raw, kind);
            assert_eq!(preview.code.language, "diff", "{raw}");
            assert_eq!(&*preview.code.source, raw);
            assert_eq!(&*preview.original, raw);
        }
    }
}

#[test]
fn complete_apply_patch_inputs_are_shown_without_inventing_deleted_file_contents() {
    for raw in [
        "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-old\n+new\n*** End Patch\n",
        "*** Begin Patch\n*** Update File: src/a.rs\n-old\n+new\n*** End Patch\n",
        "*** Begin Patch\n*** Add File: new.txt\n+new\n*** Delete File: old.txt\n*** End Patch",
        "*** Begin Patch\n*** Update File: old.rs\n*** Move to: new.rs\n@@ section\n same\n-old\n+new\n*** End of File\n*** End Patch\n",
        "*** Begin Patch\n*** Delete File: unknown.txt\n*** End Patch\n",
        "*** Begin Patch\n*** Add File: empty.txt\n*** End Patch\n",
    ] {
        let preview = text_preview(raw, ToolKind::Edit);
        assert_eq!(preview.code.language, "diff", "{raw}");
        assert_eq!(&*preview.code.source, raw);
        assert_eq!(&*preview.original, raw);
    }
}

#[test]
fn incomplete_malformed_and_unanchored_patches_remain_original_text() {
    for raw in [
        "-old\n+new\n",
        "--- a\n+++ b\n@@ -1,2 +1 @@\n-old\n+new\n",
        "--- a\n+++ b\n@@ -+1 +1 @@\n-old\n+new\n",
        "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n--- missing\n+++ unfinished\n",
        "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\nindex trailing nonsense\n",
        "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file\n\\ No newline at end of file\n",
        "*** Begin Patch\n*** Update File: a\n@@\n-old\n+new\n",
        "*** Begin Patch\n*** Update File: a\n@@\n context only\n*** End Patch\n",
        "*** Begin Patch\n*** Add File: a\nnot an added line\n*** End Patch\n",
    ] {
        let preview = text_preview(raw, ToolKind::Edit);
        assert_ne!(preview.code.language, "diff", "{raw}");
        assert_eq!(&*preview.code.source, raw);
        assert_eq!(&*preview.original, raw);
    }
}
