use super::*;

#[test]
fn saved_tags_load_back_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = file(dir.path());
    assert_eq!(load(&path), ComputerTags::default());

    let tags = ComputerTags::new(["work"], 7);
    save(&path, &tags).unwrap();
    assert_eq!(load(&path), tags);

    fs::write(&path, r#"{"tags":["a","A","  b "],"updated_ms":3}"#).unwrap();
    assert_eq!(load(&path).tags, ["a", "b"]);
}

#[test]
fn a_damaged_file_costs_only_the_labels() {
    let dir = tempfile::tempdir().unwrap();
    let path = file(dir.path());
    fs::write(&path, b"not json").unwrap();
    assert_eq!(load(&path), ComputerTags::default());
}

#[test]
fn with_no_daemon_the_file_takes_the_offer() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");

    assert_eq!(exchange(&state_dir, None).unwrap(), ComputerTags::default());
    assert!(!state_dir.exists(), "reading leaves nothing behind");

    let offer = ComputerTags::new(["work"], 10);
    assert_eq!(exchange(&state_dir, Some(offer.clone())).unwrap(), offer);
    assert_eq!(load(&file(&state_dir)), offer);

    let stale = ComputerTags::new(["personal"], 5);
    assert_eq!(exchange(&state_dir, Some(stale)).unwrap(), offer);
    assert_eq!(exchange(&state_dir, None).unwrap(), offer);
}

#[test]
fn processes_offering_at_once_leave_the_latest() {
    let dir = tempfile::tempdir().unwrap();
    let workers: Vec<_> = (1..=32_u64)
        .map(|edit| {
            let dir = dir.path().to_owned();
            std::thread::spawn(move || {
                let offer = ComputerTags::new([format!("tag{edit}")], edit);
                exchange(&dir, Some(offer)).unwrap();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(load(&file(dir.path())), ComputerTags::new(["tag32"], 32));
}
