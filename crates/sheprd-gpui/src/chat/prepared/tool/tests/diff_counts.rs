use super::*;

fn summary(old: &str, new: &str) -> ToolSummary {
    summarize(
        &[ToolContent::Diff {
            path: "file.rs".into(),
            old: Some(old.into()),
            new: new.into(),
        }],
        ToolKind::Edit,
        &mut Visuals::default(),
    )
}

#[test]
fn glance_counts_match_reorders_newlines_and_header_like_changed_text() {
    for (old, new, expected, removed, added) in [
        ("one\ntwo\n", "two\none\n", (1, 1), "-one\n", "+one\n"),
        ("same", "same\n", (1, 1), "-same\n", "+same\n"),
        ("same\n", "same", (1, 1), "-same\n", "+same\n"),
        ("--old\n", "++new\n", (1, 1), "---old\n", "+++new\n"),
    ] {
        let summary = summary(old, new);
        assert_eq!((summary.diffs[0].added, summary.diffs[0].removed), expected);
        let source = &summary.previews[0].code.source;
        assert!(source.contains(removed) && source.contains(added));
    }
    let same = summary("same\n", "same\n");
    assert_eq!((same.diffs[0].added, same.diffs[0].removed), (0, 0));
}

#[test]
fn coarse_preview_counts_cover_all_generated_operations_without_display_row_limits() {
    let old = (0..600).map(|i| format!("old {i}\n")).collect::<String>();
    let new = (0..600).map(|i| format!("new {i}\n")).collect::<String>();
    let summary = summary(&old, &new);
    assert_eq!(
        (summary.diffs[0].added, summary.diffs[0].removed),
        (600, 600)
    );
    let source = &summary.previews[0].code.source;
    assert!(source.contains("-old 599\n") && source.contains("+new 599\n"));
}

#[test]
fn unprepared_changes_keep_their_source_without_guessing_numeric_totals() {
    for old in ["x".repeat(STRUCTURED_BYTES), "x\n".repeat(2_049)] {
        let summary = summary(&old, "new\n");
        assert!(summary.diffs.is_empty());
        assert!(summary.previews[0].before.is_some());
        assert!(summary.previews[0].original.contains(&old));
    }
    let mut content = vec![
        ToolContent::Text {
            text: "output".into()
        };
        MAX_PARTS
    ];
    content.push(ToolContent::Diff {
        path: "overflow.rs".into(),
        old: Some("old\n".into()),
        new: "new\n".into(),
    });
    let summary = summarize(&content, ToolKind::Edit, &mut Visuals::default());
    assert!(summary.diffs.is_empty());
    assert!(summary.previews[MAX_PARTS].original.contains("overflow.rs"));
    assert!(summary.previews[MAX_PARTS].before.is_some());
}

#[test]
fn a_whole_new_file_counts_its_lines_at_any_size() {
    let new = "line\n".repeat(STRUCTURED_BYTES);
    let summary = summarize(
        &[ToolContent::Diff {
            path: "file.rs".into(),
            old: None,
            new: new.clone(),
        }],
        ToolKind::Edit,
        &mut Visuals::default(),
    );
    assert_eq!(counts(&summary.diffs), [("file.rs", STRUCTURED_BYTES, 0)]);
}

#[test]
fn counted_edits_read_the_same_as_their_diff() {
    let summary = summary("a\nb\nc\n", "a\nB\nc\nd\n");
    assert_eq!(counts(&summary.diffs), [("file.rs", 2, 1)]);
}
