use super::*;

#[test]
fn counting_listed_checkouts_reprobes_dirty_answers_and_keeps_the_backoff() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let listed = [input("feature"), input("other")];
    peer.git.track(Some(input("feature")), true, now);
    peer.git.track_listed(listed.clone(), true, now);
    peer.git.poll(now);
    let (requested, _) = peer.request();
    peer.complete(requested, Completion::Status(Ok(status(5, 1, 0))), now);
    // The focused checkout's counts come from its own status.
    assert_eq!(
        peer.git.counts("/repo/.git", "feature"),
        Some(status(5, 1, 0))
    );
    peer.git.poll(now);
    let (requested, job) = peer.request();
    assert!(
        matches!(job, Job::Dirty),
        "dirt only, until counts are wanted"
    );
    peer.complete(requested, Completion::Dirty(Ok(true)), now);
    assert_eq!(peer.git.counts("/repo/.git", "other"), None);

    // Turning counting on asks again at the next scan, not after the refresh.
    peer.git.set_counted(true);
    let next = now + SCAN_INTERVAL;
    peer.git.track_listed(listed.clone(), true, next);
    peer.git.poll(next);
    let (requested, job) = peer.request();
    assert_eq!(requested, input("other"));
    assert!(matches!(job, Job::Counts));
    peer.complete(
        requested,
        Completion::Counts(Ok(counted(42, 7, true))),
        next,
    );
    assert_eq!(
        peer.git.counts("/repo/.git", "other"),
        Some(status(42, 7, 0))
    );
    assert_eq!(peer.git.dirty("/repo/.git", "other"), Some(true));

    // A counted answer waits out the usual refresh.
    let early = next + PROBE_REFRESH - Duration::from_secs(1);
    peer.git.track_listed(listed.clone(), true, early);
    peer.git.poll(early);
    assert!(peer.incoming.try_recv().is_err());

    // A failure is unknown, and backs off rather than retrying every scan.
    let due = next + PROBE_REFRESH;
    peer.git.track_listed(listed.clone(), true, due);
    peer.git.poll(due);
    let (requested, job) = peer.request();
    assert!(matches!(job, Job::Counts));
    peer.complete(
        requested,
        Completion::Counts(Err(Error::NoCheckout.into())),
        due,
    );
    assert_eq!(peer.git.counts("/repo/.git", "other"), None);
    assert_eq!(peer.git.dirty("/repo/.git", "other"), None);
    let later = due + SCAN_INTERVAL;
    peer.git.track_listed(listed.clone(), true, later);
    peer.git.poll(later);
    assert!(peer.incoming.try_recv().is_err(), "a failure waits longer");

    // Counted answers still mark dirt once counting is off again.
    peer.git.set_counted(false);
    // A binary edit is dirty with no line to count; the mark stays.
    peer.git
        .seed_counts(input("other"), counted(0, 0, true), later);
    assert_eq!(peer.git.dirty("/repo/.git", "other"), Some(true));
    assert_eq!(
        peer.git.counts("/repo/.git", "other"),
        Some(status(0, 0, 0))
    );
}

fn counted(additions: u64, deletions: u64, dirty: bool) -> Counted {
    Counted {
        lines: status(additions, deletions, 0),
        dirty,
    }
}

#[test]
fn a_counted_probe_reads_lines_against_head_and_keeps_uncountable_changes_dirty() {
    let directory = tempfile::tempdir().unwrap();
    let checkout = directory.path().to_str().unwrap();
    let command = |args: &[&str]| {
        let result = Command::new("git")
            .args(["-C", checkout, "-c", "user.name=Test"])
            .args([
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    let input = Input {
        repo_key: Some(directory.path().join(".git").to_str().unwrap().to_owned()),
        branch: "test".into(),
        checkout: Some(checkout.into()),
    };
    let probe = || {
        let Completion::Counts(result) = execute(&input, Job::Counts, &|| false) else {
            panic!("a counted probe answers with counts");
        };
        result.unwrap()
    };
    command(&["init", "-b", "test"]);
    let write = |name: &str, contents: &[u8]| {
        std::fs::write(directory.path().join(name), contents).unwrap();
    };
    // No commit yet, so no HEAD to count against: still marked dirty.
    write("tracked", b"one\ntwo\nthree\n");
    write("logo.bin", b"\0\x01\x02binary");
    assert_eq!(probe(), counted(0, 0, true));
    command(&["add", "tracked", "logo.bin"]);
    command(&["commit", "--no-verify", "-m", "initial"]);
    assert_eq!(probe(), counted(0, 0, false));
    // A binary edit changes no countable line, yet is work to commit.
    write("logo.bin", b"\0\x03\x04binary");
    assert_eq!(probe(), counted(0, 0, true));
    write("tracked", b"one\n2\n3\nfour\n");
    write("new", b"new\n");
    assert_eq!(probe(), counted(3, 2, true));
}
