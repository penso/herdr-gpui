use super::super::changes::*;

fn line(text: &str) -> DiffLine {
    DiffLine {
        kind: LineKind::Added,
        old: None,
        new: Some(1),
        text: text.into(),
    }
}

fn file(path: &str) -> ChangedFile {
    ChangedFile {
        path: path.into(),
        ..ChangedFile::default()
    }
}

#[test]
fn a_long_diff_is_cut_at_the_line_limit_and_says_so() {
    let diff = FileDiff::bounded("a.rs", (0..MAX_DIFF_LINES + 1).map(|_| line("x")));
    assert_eq!(diff.lines.len(), MAX_DIFF_LINES);
    assert!(diff.truncated);
    assert!(diff.valid());
}

#[test]
fn a_diff_of_exactly_the_limit_is_whole() {
    let diff = FileDiff::bounded("a.rs", (0..MAX_DIFF_LINES).map(|_| line("x")));
    assert!(!diff.truncated);
}

#[test]
fn a_wide_diff_is_cut_at_the_byte_limit_and_says_so() {
    let wide = "é".repeat(MAX_LINE_CHARS);
    let diff = FileDiff::bounded("a.rs", (0..MAX_DIFF_LINES).map(|_| line(&wide)));
    assert!(diff.truncated);
    assert!(diff.lines.len() < MAX_DIFF_LINES);
    assert!(diff.lines.iter().map(|l| l.text.len()).sum::<usize>() <= MAX_DIFF_BYTES);
    assert!(diff.valid());
}

#[test]
fn a_long_line_ends_in_an_ellipsis_within_its_limit() {
    let diff = FileDiff::bounded("a.rs", [line(&"x".repeat(MAX_LINE_CHARS * 4))]);
    let text = &diff.lines[0].text;
    assert_eq!(text.chars().count(), MAX_LINE_CHARS);
    assert!(text.ends_with('\u{2026}'));
    assert!(!diff.truncated);
}

#[test]
fn a_list_counts_every_file_but_sends_only_the_first() {
    let list = FileList::bounded(None, (0..MAX_FILES + 7).map(|n| file(&format!("{n}.rs"))));
    assert_eq!(list.files.len(), MAX_FILES);
    assert_eq!(list.total, MAX_FILES + 7);
    assert!(list.valid());
}

#[test]
fn a_path_too_long_to_send_is_counted_and_left_out() {
    let list = FileList::bounded(None, [file(&"p".repeat(MAX_PATH + 1)), file("a.rs")]);
    assert_eq!(list.total, 2);
    assert_eq!(list.files, [file("a.rs")]);
}

#[test]
fn a_note_cuts_a_long_multibyte_path_between_characters() {
    let diff = FileDiff::note(&"é".repeat(MAX_PATH), Shown::TooLarge);
    assert!(diff.path.len() <= MAX_PATH);
    assert!(diff.valid());
}

#[test]
fn answers_past_the_bounds_are_refused() {
    let lines = vec![line("x"); MAX_DIFF_LINES + 1];
    let too_long = FileDiff {
        path: "a.rs".into(),
        lines,
        ..FileDiff::default()
    };
    assert!(!Answer::Diff(too_long).valid());
    let wide = FileDiff {
        path: "a.rs".into(),
        lines: vec![line(&"x".repeat(MAX_LINE_CHARS + 1))],
        ..FileDiff::default()
    };
    assert!(!Answer::Diff(wide).valid());
    let many = FileList {
        files: vec![file("a.rs"); MAX_FILES + 1],
        total: MAX_FILES + 1,
        base: None,
    };
    assert!(!Answer::Files(many).valid());
    let undercounted = FileList {
        files: vec![file("a.rs")],
        total: 0,
        base: None,
    };
    assert!(!Answer::Files(undercounted).valid());
    let check = Check {
        name: "test".into(),
        state: CheckState::Failed,
        job: None,
        url: None,
    };
    let checks = Checks {
        number: Some(1),
        checks: vec![check; MAX_CHECKS + 1],
    };
    assert!(!Answer::Checks(checks).valid());
    let log = CheckLog {
        excerpt: "x".repeat(MAX_EXCERPT + 1),
    };
    assert!(!Answer::CheckLog(log).valid());
}

#[test]
fn a_failure_from_a_newer_sheprd_reads_as_unreadable() {
    let failure: Failure = serde_json::from_str(r#""something_new""#).unwrap();
    assert_eq!(failure, Failure::Unreadable);
}

#[test]
fn asks_and_answers_cross_the_socket_unchanged() {
    let ask = Ask {
        id: AskId(4),
        boot_id: "b".into(),
        workspace_id: "w".into(),
        wanted: Wanted::Diff {
            scope: Scope::Branch,
            path: "a.rs".into(),
        },
    };
    let text = serde_json::to_string(&ask).unwrap();
    assert_eq!(serde_json::from_str::<Ask>(&text).unwrap(), ask);
    let answer = Answer::Failed(Failure::NoBase);
    let text = serde_json::to_string(&answer).unwrap();
    assert_eq!(serde_json::from_str::<Answer>(&text).unwrap(), answer);
}
