use super::*;

fn kinds(source: &str) -> Vec<Kind> {
    rows(source, MAX_ROWS)
        .into_iter()
        .map(|row| row.kind)
        .collect()
}

#[test]
fn unified_headers_are_neutral_and_counts_end_each_hunk() {
    let source = "--- a/first.rs\n+++ b/first.rs\n@@ -2,2 +2,2 @@ function\n same\n-old\n+new\n--- a/next.py\n+++ b/next.py\n@@ -0,0 +1 @@\n+added\n+outside";
    assert_eq!(
        kinds(source),
        [
            Kind::Meta,
            Kind::Meta,
            Kind::Meta,
            Kind::Context,
            Kind::Removed,
            Kind::Added,
            Kind::Meta,
            Kind::Meta,
            Kind::Meta,
            Kind::Added,
            Kind::Context,
        ]
    );
    let classified = rows(source, MAX_ROWS);
    assert_eq!(classified[4].path, Some("a/first.rs"));
    assert_eq!(classified[5].path, Some("b/first.rs"));
    assert_eq!(classified[9].path, Some("b/next.py"));
    assert_eq!(classified[10].prefix, 0);
}

#[test]
fn header_like_content_keeps_its_actual_change_kind_inside_a_hunk() {
    let source = "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n--- old content\n+++ new content\n";
    let classified = rows(source, MAX_ROWS);
    assert_eq!(classified[3].kind, Kind::Removed);
    assert_eq!(classified[4].kind, Kind::Added);
    assert_eq!(classified[3].path, Some("a/file.rs"));
    assert_eq!(classified[4].path, Some("b/file.rs"));
    assert_eq!((classified[3].prefix, classified[4].prefix), (1, 1));
}

#[test]
fn generated_fragments_have_no_invented_counts_and_keep_unicode_source_offsets() {
    let source = "--- src/বাংলা.rs\n+++ src/বাংলা.rs\n@@ Provided text @@\n-\tlet α = 1;\n+\tlet α = 2;\n\\ No newline at end of supplied text\n@@ Provided text @@\n unchanged\n";
    let classified = rows(source, MAX_ROWS);
    assert_eq!(classified[3].kind, Kind::Removed);
    assert_eq!(classified[4].kind, Kind::Added);
    assert_eq!(classified[5].kind, Kind::Meta);
    assert_eq!(classified[7].kind, Kind::Context);
    for (line, row) in source.lines().zip(&classified) {
        assert!(line.is_char_boundary(row.prefix));
        if row.prefix == 1 {
            assert_eq!(row.path, Some("src/বাংলা.rs"));
        }
    }
    let lines: Vec<_> = source.lines().collect();
    assert_eq!(&lines[3][classified[3].prefix..], "\tlet α = 1;");
    assert_eq!(&lines[4][classified[4].prefix..], "\tlet α = 2;");
    assert_eq!(classified[5].prefix, 0);
}

#[test]
fn apply_patch_handles_added_files_moves_updates_and_terminal_metadata() {
    let source = "*** Begin Patch\n*** Add File: src/new.rs\n+fn new() {}\n*** Update File: src/old.rs\n*** Move to: src/moved.rs\n@@ impl Example\n-old\n+new\n*** End of File\n+outside\n*** Delete File: src/deleted.rs\n*** End Patch\n+after patch";
    let classified = rows(source, MAX_ROWS);
    assert_eq!(classified[2].kind, Kind::Added);
    assert_eq!(classified[2].path, Some("src/new.rs"));
    assert_eq!(classified[6].kind, Kind::Removed);
    assert_eq!(classified[6].path, Some("src/old.rs"));
    assert_eq!(classified[7].kind, Kind::Added);
    assert_eq!(classified[7].path, Some("src/moved.rs"));
    for index in [0, 1, 3, 4, 5, 8, 10, 11] {
        assert_eq!(classified[index].kind, Kind::Meta);
        assert_eq!(classified[index].prefix, 0);
    }
    for index in [9, 12] {
        assert_eq!(classified[index], unknown());
    }
}

#[test]
fn apply_patch_first_update_hunk_can_omit_the_context_marker() {
    let source = "*** Begin Patch\n*** Update File: src/file.rs\n-old\n+new\n*** End Patch";
    let classified = rows(source, MAX_ROWS);
    assert_eq!(classified[2].kind, Kind::Removed);
    assert_eq!(classified[3].kind, Kind::Added);
    assert_eq!(classified[2].path, Some("src/file.rs"));
    assert_eq!(classified[3].path, Some("src/file.rs"));
}

#[test]
fn malformed_or_unrecognized_fragments_do_not_color_following_signs() {
    for header in [
        "@@ example @@",
        "@@ Provided text @@ extra",
        "@@ -1 +1",
        "@@ -1,-1 +1 @@",
        "@@ -1,+2 +1 @@",
        "@@ -1,2,3 +1 @@",
        "@@ -1 +4294967296 @@",
        "@@ - +1 @@",
    ] {
        let source = format!("{header}\n+ordinary\n-ordinary");
        let classified = rows(&source, MAX_ROWS);
        assert_eq!(classified[1], unknown(), "{header}");
        assert_eq!(classified[2], unknown(), "{header}");
    }
    assert_eq!(kinds("+ordinary\n-ordinary\n context"), [Kind::Context; 3]);
}

#[test]
fn malformed_content_does_not_leave_a_hunk_open() {
    let classified = rows(
        "@@ -1,2 +1,2 @@\nnot a content row\n+outside\n-old",
        MAX_ROWS,
    );
    assert_eq!(&classified[1..], &[unknown(); 3]);
    let classified = rows("@@ -0,0 +0,0 @@\n+outside", MAX_ROWS);
    assert_eq!(classified[1], unknown());
}

#[test]
fn no_newline_marker_does_not_consume_unified_counts() {
    let source = "@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n+outside";
    assert_eq!(
        kinds(source),
        [
            Kind::Meta,
            Kind::Removed,
            Kind::Meta,
            Kind::Added,
            Kind::Meta,
            Kind::Context
        ]
    );
}

#[test]
fn git_file_boundary_resets_partial_counts_and_path_hints() {
    let source = "--- a/first.rs\n+++ b/first.rs\n@@ -1,2 +1,2 @@\n-old\ndiff --git a/next.py b/next.py\n+not yet a hunk\n--- a/next.py\n+++ b/next.py\n@@ -0,0 +1 @@\n+new";
    let classified = rows(source, MAX_ROWS);
    assert_eq!(classified[4].kind, Kind::Meta);
    assert_eq!(classified[4].path, None);
    assert_eq!(classified[5], unknown());
    assert_eq!(classified[9].path, Some("b/next.py"));
}

#[test]
fn native_filename_before_a_counted_hunk_supplies_both_grammar_hints() {
    let source = "src/বাংলা.rs\r\n@@ -1 +1 @@\r\n-old\r\n+new\r\n";
    let classified = rows(source, MAX_ROWS);
    assert_eq!(classified[0].kind, Kind::Meta);
    assert_eq!(classified[0].prefix, 0);
    assert_eq!(classified[2].path, Some("src/বাংলা.rs"));
    assert_eq!(classified[3].path, Some("src/বাংলা.rs"));
    assert_eq!(classified[2].kind, Kind::Removed);
    assert_eq!(classified[3].kind, Kind::Added);
    for source in [
        "name.rs\n@@ -invalid +1 @@\n+line".to_owned(),
        "name.rs\n@@ Provided text @@\n+line".to_owned(),
        "name.rs\n@@ -0,0 +0,0 @@".to_owned(),
        format!("{}\n@@ -1 +1 @@", "x".repeat(MAX_HEADER_BYTES + 1)),
        "--- name.rs\n@@ -1 +1 @@".to_owned(),
    ] {
        assert_eq!(initial_path(&source), None, "{source}");
    }
    assert_eq!(rows(source, 1), [unknown()]);
}

#[test]
fn file_hints_handle_crlf_timestamps_and_quoted_paths_without_reading_them() {
    let source =
        "--- /dev/null\r\n+++ \"b/has space.rs\"\t2026-01-01\r\n@@ -0,0 +1 @@\r\n+let α = 1;\r\n";
    let classified = rows(source, MAX_ROWS);
    assert_eq!(classified[0].path, None);
    assert_eq!(classified[3].path, Some("b/has space.rs"));
    assert_eq!(classified[3].kind, Kind::Added);
    assert_eq!(path(&"x".repeat(MAX_HEADER_BYTES + 1)), None);
}

#[test]
fn classifier_stops_at_line_and_byte_budgets_without_interpreting_a_partial_header() {
    let source = format!("@@ Provided text @@\n{}", "+line\n".repeat(MAX_ROWS + 10));
    assert!(rows(&source, 0).is_empty());
    assert_eq!(rows(&source, 3).len(), 3);
    assert_eq!(rows(&source, usize::MAX).len(), MAX_ROWS);
    let long = "x".repeat(super::super::MAX_CODE_BYTES + 1);
    assert_eq!(rows(&long, MAX_ROWS), [unknown()]);
    // A multibyte scalar crossing the scan bound is never sliced midway.
    let unicode = format!(
        "{}α\n@@ Provided text @@\n+after",
        "x".repeat(super::super::MAX_CODE_BYTES - 1)
    );
    assert_eq!(rows(&unicode, MAX_ROWS), [unknown()]);
    let header = format!("@@ {}", "x".repeat(MAX_HEADER_BYTES));
    assert_eq!(counts(&header), None);
}
