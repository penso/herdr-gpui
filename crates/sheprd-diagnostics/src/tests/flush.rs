use super::*;

fn writer(
    path: PathBuf,
    queued: Receiver<Queued>,
    counters: &Arc<Counters>,
) -> thread::JoinHandle<()> {
    let counters = counters.clone();
    thread::spawn(move || write_lines(path, queued, &counters))
}

#[test]
fn flushing_waits_until_earlier_lines_reach_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    let (lines, queued) = mpsc::sync_channel(8);
    let counters = Arc::new(Counters::default());
    for message in ["first", "panicked"] {
        lines.send(Queued::Line(line(message))).unwrap();
    }
    // No writer yet: the wait gives up rather than hang.
    assert!(!flush_through(&lines, Duration::from_millis(20)));

    let writer = writer(path.clone(), queued, &counters);
    assert!(flush_through(&lines, Duration::from_secs(10)));
    let mut tail = Tail::new(&path);
    tail.read().unwrap();
    let written: Vec<_> = tail.records().iter().map(|r| r.message.clone()).collect();
    assert_eq!(written, ["first", "panicked"]);

    drop(lines);
    writer.join().unwrap();
}

/// Another thread's lines landing between this thread's line and its flush
/// cannot answer for it: the marker itself is answered, behind every line.
#[test]
fn a_flush_is_answered_only_after_lines_queued_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    let (lines, queued) = mpsc::sync_channel(QUEUE);
    let counters = Arc::new(Counters::default());
    let writer = writer(path.clone(), queued, &counters);
    thread::scope(|scope| {
        for thread in 0..4 {
            let lines = lines.clone();
            let path = &path;
            scope.spawn(move || {
                for i in 0..50 {
                    let message = format!("{thread}-{i}");
                    lines.send(Queued::Line(line(&message))).unwrap();
                    assert!(flush_through(&lines, Duration::from_secs(10)));
                    let text = fs::read_to_string(path).unwrap();
                    assert!(
                        text.contains(&format!("\"{message}\"")),
                        "{message} not written"
                    );
                }
            });
        }
    });
    drop(lines);
    writer.join().unwrap();
}

#[test]
fn lines_the_writer_drops_still_answer_the_flush() {
    let dir = tempfile::tempdir().unwrap();
    // A directory where the file should be makes every append fail.
    let path = dir.path().join(FILE_NAME);
    fs::create_dir(&path).unwrap();
    let (lines, queued) = mpsc::sync_channel(8);
    let counters = Arc::new(Counters::default());
    let writer = writer(path, queued, &counters);
    lines.send(Queued::Line(line("lost"))).unwrap();
    assert!(flush_through(&lines, Duration::from_secs(10)));
    assert_eq!(counters.dropped.load(Ordering::Relaxed), 1);
    drop(lines);
    writer.join().unwrap();
}
