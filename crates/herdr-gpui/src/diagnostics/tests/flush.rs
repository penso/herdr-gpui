use super::*;

#[test]
fn flushing_waits_until_queued_lines_reach_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    let (lines, queued) = mpsc::sync_channel(8);
    let counters = Arc::new(Counters::default());
    for message in ["first", "panicked"] {
        lines.send(line(message)).unwrap();
        counters.queued.fetch_add(1, Ordering::Release);
    }
    // No writer yet: the wait gives up rather than hang.
    assert!(!counters.settle(Duration::from_millis(20)));

    let writer_counters = counters.clone();
    let writer_path = path.clone();
    let writer = thread::spawn(move || write_lines(writer_path, queued, &writer_counters));
    assert!(counters.settle(Duration::from_secs(10)));
    let mut tail = Tail::new(&path);
    tail.read().unwrap();
    let written: Vec<_> = tail.records().iter().map(|r| r.message.clone()).collect();
    assert_eq!(written, ["first", "panicked"]);

    drop(lines);
    writer.join().unwrap();
}

#[test]
fn lines_the_writer_drops_still_settle() {
    let dir = tempfile::tempdir().unwrap();
    // A directory where the file should be makes every append fail.
    let path = dir.path().join(FILE_NAME);
    fs::create_dir(&path).unwrap();
    let (lines, queued) = mpsc::sync_channel(8);
    let counters = Arc::new(Counters::default());
    lines.send(line("lost")).unwrap();
    counters.queued.fetch_add(1, Ordering::Release);
    drop(lines);
    write_lines(path, queued, &counters);
    assert!(counters.settle(Duration::ZERO));
    assert_eq!(counters.dropped.load(Ordering::Relaxed), 1);
}
