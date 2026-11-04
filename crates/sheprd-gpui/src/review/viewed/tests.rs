#![allow(clippy::unwrap_used)]
use super::*;

fn files(paths: &[&str]) -> HashMap<String, Counts> {
    paths
        .iter()
        .map(|path| ((*path).to_owned(), (Some(1), Some(2))))
        .collect()
}

#[test]
fn marks_are_kept_per_checkout_and_branch_and_empty_forgets() {
    let mut viewed = Viewed::default();
    assert!(viewed.set("/work/repo", "feat", &files(&["a.rs", "b.rs"])));
    assert!(
        !viewed.set("/work/repo", "feat", &files(&["b.rs", "a.rs"])),
        "same marks"
    );
    assert_eq!(viewed.get("/work/repo", "feat"), files(&["a.rs", "b.rs"]));
    assert!(viewed.get("/work/repo", "main").is_empty());
    assert!(viewed.get("/work/other", "feat").is_empty());
    assert!(viewed.set("/work/repo", "feat", &HashMap::new()));
    assert!(viewed.get("/work/repo", "feat").is_empty());
    assert!(
        !viewed.set("/work/repo", "absent", &HashMap::new()),
        "nothing to forget"
    );
}

#[test]
fn the_least_recently_marked_checkouts_go_first_beyond_the_bounds() {
    let mut viewed = Viewed::default();
    for index in 0..MAX_CHECKOUTS + 2 {
        viewed.set("/work/repo", &format!("b{index}"), &files(&["a.rs"]));
    }
    assert_eq!(viewed.checkouts.len(), MAX_CHECKOUTS);
    assert!(viewed.get("/work/repo", "b0").is_empty());
    assert!(!viewed.get("/work/repo", "b2").is_empty());

    let many: HashMap<_, _> = (0..MAX_FILES)
        .map(|index| (format!("f{index}"), (None, None)))
        .collect();
    viewed.set("/work/big", "main", &many);
    assert_eq!(viewed.checkouts.len(), 1, "older checkouts make room");
    assert_eq!(viewed.get("/work/big", "main").len(), MAX_FILES);

    let long = "x".repeat(MAX_FIELD_BYTES + 1);
    assert!(!viewed.set(&long, "main", &files(&["a.rs"])));
    viewed.set("/work/repo", "main", &files(&["a.rs", &long]));
    assert_eq!(
        viewed.get("/work/repo", "main"),
        files(&["a.rs"]),
        "a long path is not kept"
    );
}

#[test]
fn saved_marks_load_back_and_damaged_files_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gpui").join(FILE);
    let mut viewed = Viewed::at(Some(path.clone()));
    viewed.set("/work/repo", "feat", &files(&["src/main.rs"]));
    viewed.writer.take().unwrap().finish();
    let reopened = Viewed::at(Some(path.clone()));
    assert_eq!(reopened.get("/work/repo", "feat"), files(&["src/main.rs"]));

    std::fs::write(
        &path,
        br#"{"checkouts":[{"place":"","branch":"b","files":[]}]}"#,
    )
    .unwrap();
    assert!(matches!(
        parse(&std::fs::read(&path).unwrap()),
        Err(crate::Error::Review(super::super::Error::InvalidViewed))
    ));
    assert!(
        Viewed::at(Some(path)).checkouts.is_empty(),
        "a damaged file starts empty"
    );
}

#[test]
fn a_review_is_keyed_by_its_directory_else_its_repository() {
    let input = |checkout: Option<&str>, repo_key: Option<&str>| crate::pull_request::Input {
        checkout: checkout.map(Into::into),
        repo_key: repo_key.map(Into::into),
        branch: "feat".into(),
    };
    assert_eq!(
        key(&input(Some("/work/repo"), Some("/work/repo/.git"))),
        Some(("/work/repo", "feat"))
    );
    assert_eq!(
        key(&input(None, Some("/work/repo/.git"))),
        Some(("/work/repo/.git", "feat"))
    );
    assert_eq!(key(&input(None, None)), None);
}

#[test]
fn marks_change_one_file_at_a_time_and_unmark_only_the_same_counts() {
    let mut viewed = Viewed::default();
    assert!(viewed.mark("/work/repo", "feat", "a.rs", (Some(1), Some(2))));
    assert!(viewed.mark("/work/repo", "feat", "b.rs", (Some(1), Some(2))));
    assert_eq!(viewed.get("/work/repo", "feat"), files(&["a.rs", "b.rs"]));
    assert!(
        !viewed.unmark("/work/repo", "feat", [("a.rs", (Some(5), None))]),
        "a newer mark stays"
    );
    assert!(viewed.unmark("/work/repo", "feat", [("a.rs", (Some(1), Some(2)))]));
    assert_eq!(viewed.get("/work/repo", "feat"), files(&["b.rs"]));
}
