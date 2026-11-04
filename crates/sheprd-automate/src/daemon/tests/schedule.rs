//! Triggers fire on time while the daemon ticks every couple of seconds.
use super::*;

/// A daemon over the fixture, without the serve loop, and where its runs
/// report when they end.
fn daemon(fixture: &Fixture) -> (Daemon<'_>, mpsc::Receiver<(String, i64, Result<Ended>)>) {
    let (done, finished) = mpsc::channel();
    let settings = fixture.settings();
    let daemon = Daemon {
        paths: &fixture.paths,
        store: fixture.store(),
        log: Logbook::new(fixture.paths.log()),
        settings,
        herdr: fixture.herdr.clone(),
        loaded: HashMap::new(),
        live: HashMap::new(),
        slots: Arc::default(),
        done,
    };
    (daemon, finished)
}

#[test]
fn an_interval_fires_once_its_interval_has_passed_since_it_was_turned_on() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "often",
        "herdr.every(\"1m\", function() notify(\"tick\") end)\n",
    );
    let (mut daemon, finished) = daemon(&fixture);
    daemon.store.add_repo(&repo.to_string_lossy(), 1).unwrap();
    let path = script.to_string_lossy().into_owned();
    let hash = Source::read(&script).unwrap().hash;
    daemon.store.approve(&path, &hash, "test", 1).unwrap();

    let start = 1_000_000;
    daemon.refresh(start).unwrap();
    // Ticks every two seconds, as the serve loop does, must not keep
    // pushing the first firing away.
    for now in (start..start + 60).step_by(2) {
        daemon.schedule(&path, false, now).unwrap();
        assert!(daemon.live.is_empty(), "fired early at +{}", now - start);
    }
    daemon.schedule(&path, false, start + 60).unwrap();
    assert!(daemon.live.contains_key(&path), "did not fire at +60");
    let (_, run, ended) = finished.recv_timeout(Duration::from_secs(20)).unwrap();
    daemon.live.remove(&path);
    daemon.finish(&path, run, ended).unwrap();

    // The next firing counts from that one.
    for now in (start + 61..start + 120).step_by(2) {
        daemon.schedule(&path, false, now).unwrap();
        assert!(daemon.live.is_empty(), "fired again at +{}", now - start);
    }
    daemon.schedule(&path, false, start + 120).unwrap();
    assert!(daemon.live.contains_key(&path), "did not fire at +120");
    let (_, run, ended) = finished.recv_timeout(Duration::from_secs(20)).unwrap();
    daemon.finish(&path, run, ended).unwrap();
}

#[test]
fn run_now_reads_a_script_approved_since_the_last_scan() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    let (mut daemon, finished) = daemon(&fixture);
    daemon.store.add_repo(&repo.to_string_lossy(), 1).unwrap();
    daemon.refresh(1_000).unwrap();
    // Written and approved after that scan, then run at once.
    let script = fixture.script(
        &repo,
        "fresh",
        "herdr.manual(function() notify(\"hi\") end)\n",
    );
    let path = script.to_string_lossy().into_owned();
    let hash = Source::read(&script).unwrap().hash;
    daemon.store.approve(&path, &hash, "test", 1_001).unwrap();
    daemon
        .store
        .push_request(
            &RequestKind::RunNow {
                path: path.clone(),
                trigger: None,
            },
            1_002,
        )
        .unwrap();
    assert!(!daemon.handle_requests(1_002).unwrap());
    assert!(daemon.live.contains_key(&path), "the run did not start");
    let (_, run, ended) = finished.recv_timeout(Duration::from_secs(20)).unwrap();
    daemon.finish(&path, run, ended).unwrap();
}
