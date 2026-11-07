use super::*;

fn tagged(records: &[(&str, &[&str])]) -> Tags {
    Tags::tagged(records)
}

#[test]
fn every_tag_is_listed_once_alphabetically() {
    let tags = tagged(&[("a", &["work", "Lab"]), ("b", &["Work", "client"])]);
    assert_eq!(tags.all(), ["client", "Lab", "work"]);
    assert!(tags.any());
    assert!(!Tags::default().any());
}

#[test]
fn a_device_lists_once_under_its_first_tag_or_the_filter() {
    let tags = tagged(&[("a", &["personal", "work"]), ("b", &[])]);
    assert_eq!(tags.section("a", None), Some("personal"));
    assert_eq!(tags.section("a", Some("WORK")), Some("WORK"));
    assert_eq!(tags.section("a", Some("lab")), Some("personal"));
    assert_eq!(tags.section("b", None), None);
    assert_eq!(tags.section("missing", None), None);
}

#[test]
fn sections_sort_alphabetically_with_untagged_last() {
    let tags = tagged(&[("a", &["work"]), ("b", &["personal"]), ("c", &[])]);
    let all = tags.all();
    let rank = |id| tags.rank(id, None, &all);
    assert!(rank("b") < rank("a"));
    assert!(rank("a") < rank("c"));
}

#[test]
fn moving_replaces_the_section_and_keeps_the_rest() {
    let tags = tagged(&[("a", &["work", "lab"]), ("b", &["Personal"])]);
    assert_eq!(tags.moved_to("a", Some("personal")), ["Personal", "lab"]);
    assert_eq!(tags.moved_to("a", Some("lab")), ["lab"]);
    assert_eq!(tags.moved_to("a", None), Vec::<String>::new());
    assert_eq!(tags.moved_to("c", Some("new")), ["new"]);
}

#[test]
fn toggling_adds_and_removes_one_tag() {
    let tags = tagged(&[("a", &["work"]), ("b", &["Lab"])]);
    assert_eq!(tags.toggled("a", "lab"), ["work", "Lab"]);
    assert_eq!(tags.toggled("a", "WORK"), Vec::<String>::new());
}

#[test]
fn an_edit_that_changes_nothing_is_not_an_edit() {
    let mut tags = tagged(&[("a", &["work"])]);
    assert!(!tags.edit("a", vec!["work".into()], 50));
    assert_eq!(tags.offer("a").updated_ms, 10);
    assert!(tags.edit("a", vec!["lab".into()], 50));
    assert_eq!(tags.offer("a").updated_ms, 50);
}

#[test]
fn the_answer_replaces_the_copy_it_was_offered_for() {
    let mut tags = tagged(&[("a", &["work"])]);
    let stood = ComputerTags::new(["personal"], 20);
    assert!(tags.settle("a", 10, stood.clone()));
    assert_eq!(tags.offer("a"), stood);
    assert!(!tags.settle("a", 20, stood));
}

#[test]
fn an_edit_made_during_an_exchange_survives_its_answer() {
    let mut tags = tagged(&[("a", &["work"])]);
    tags.edit("a", vec!["lab".into()], 30);
    assert!(!tags.settle("a", 10, ComputerTags::new(["work"], 10)));
    assert_eq!(tags.of("a"), ["lab"]);
}

#[test]
fn a_device_never_tagged_takes_its_daemons_tags_but_records_no_blank() {
    let mut tags = Tags::default();
    assert!(!tags.settle("a", 0, ComputerTags::default()));
    assert!(tags.records.is_empty());
    assert!(tags.settle("a", 0, ComputerTags::new(["work"], 5)));
    assert_eq!(tags.of("a"), ["work"]);
}

#[test]
fn the_saved_copy_reads_back_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("deep").join(FILE);
    assert_eq!(read(&file), Tags::default());

    let tags = tagged(&[("ssh:a", &["work"])]);
    write(&file, &tags).unwrap();
    assert_eq!(read(&file), tags);

    std::fs::write(&file, r#"{"ssh:a":{"tags":["a","A"],"updated_ms":3}}"#).unwrap();
    assert_eq!(read(&file).of("ssh:a"), ["a"]);

    std::fs::write(&file, b"not json").unwrap();
    assert_eq!(read(&file), Tags::default());
}

#[test]
fn a_host_with_no_daemon_prints_nothing_and_bad_answers_are_refused() {
    assert_eq!(sync::parse(b"").unwrap(), None);
    assert_eq!(sync::parse(b" \n").unwrap(), None);
    assert_eq!(
        sync::parse(br#"{"tags":["work","WORK"],"updated_ms":4}"#).unwrap(),
        Some(ComputerTags::new(["work"], 4))
    );
    assert!(matches!(sync::parse(b"nope"), Err(Error::Answer(_))));
}

#[test]
fn the_script_needs_no_install_and_reads_all_its_input() {
    assert!(sync::SCRIPT.contains("tags --offer"));
    assert!(sync::SCRIPT.contains("cat >/dev/null"));
}

#[test]
fn starters_are_offered_until_a_tag_is_in_use() {
    assert_eq!(offered(&Tags::default()), ["personal", "work"]);
    assert_eq!(offered(&tagged(&[("a", &["lab"])])), ["lab"]);
}

#[gpui::test]
fn an_edit_before_the_saved_copy_is_read_joins_it(cx: &mut gpui::TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    cx.update(|cx| {
        cx.set_global(DeviceTags {
            file: Some(file.clone()),
            ..DeviceTags::default()
        });
        DeviceTags::set(cx, "a", vec!["work".to_owned()]);
    });
    cx.run_until_parked();
    // Saving now would have put one device over everyone's tags.
    assert!(!file.exists());

    cx.update(|cx| DeviceTags::finish_loading(cx, tagged(&[("a", &["old"]), ("b", &["lab"])])));
    cx.run_until_parked();
    let tags = cx.update(|cx| DeviceTags::current(cx));
    assert_eq!(tags.of("a"), ["work"]);
    assert_eq!(tags.of("b"), ["lab"]);
    let saved = read(&file);
    assert_eq!(saved.of("a"), ["work"]);
    assert_eq!(saved.of("b"), ["lab"]);
}

#[test]
fn a_saved_edit_newer_than_an_early_one_stands() {
    let mut saved = Tags::default();
    saved.edit("a", vec!["saved".to_owned()], 2_000);
    let mut early = Tags::default();
    early.edit("a", vec!["early".to_owned()], 1_000);
    assert_eq!(saved.with_edits(&early).of("a"), ["saved"]);
}

#[gpui::test]
fn a_slow_write_does_not_hold_up_the_next_save(cx: &mut gpui::TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    let writing = Arc::new(Mutex::new(()));
    cx.update(|cx| {
        cx.set_global(DeviceTags {
            file: Some(file.clone()),
            loaded: true,
            writing: writing.clone(),
            ..DeviceTags::default()
        });
    });
    let held = writing.lock().unwrap();
    // Returns while a write has its turn, as the UI thread must.
    cx.update(|cx| DeviceTags::set(cx, "a", vec!["work".to_owned()]));
    drop(held);
    cx.run_until_parked();
    assert_eq!(read(&file).of("a"), ["work"]);
}
