use super::*;

/// A worker whose cleanup has taken its child, but has not finished. The
/// completion sender deliberately stays with the test to coordinate the race.
fn cleaning() -> (Worker, Receiver<()>, SyncSender<()>) {
    let (stop, stopped) = mpsc::sync_channel(1);
    let (finished, done) = mpsc::sync_channel(1);
    (
        Worker {
            state: Arc::new(Mutex::new(State::Starting)),
            stop,
            done,
            owned: Arc::new(Mutex::new(Owned {
                stopped: true,
                child: None,
            })),
        },
        stopped,
        finished,
    )
}

#[test]
fn replacement_waits_for_cleanup_even_after_the_owned_child_was_taken() {
    let server = HttpServer::new(200, r#"{"workbench":true,"version":"1"}"#);
    let (previous, stopped, finished) = cleaning();
    let replacement = Worker::start(
        Config {
            url: Some(server.url.clone()),
        },
        Some(previous),
    )
    .unwrap();
    // The replacement reached shutdown; hold completion beyond the old 20 ms
    // quit budget and verify that it cannot probe any server in the meantime.
    stopped.recv_timeout(Duration::from_secs(2)).unwrap();
    let premature = server.requests.recv_timeout(Duration::from_millis(100));
    finished.send(()).unwrap();
    let connected =
        premature.is_ok() || server.requests.recv_timeout(Duration::from_secs(2)).is_ok();
    replacement.shutdown();
    assert!(matches!(premature, Err(mpsc::RecvTimeoutError::Timeout)));
    assert!(connected);
}

#[test]
fn replacement_refuses_discovery_when_cleanup_did_not_report_completion() {
    let server = HttpServer::new(200, r#"{"workbench":true,"version":"1"}"#);
    let (previous, _stopped, finished) = cleaning();
    drop(finished);
    let replacement = Worker::start(
        Config {
            url: Some(server.url.clone()),
        },
        Some(previous),
    )
    .unwrap();
    replacement
        .done
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    assert!(
        matches!(replacement.state(), State::Failed(error) if matches!(*error, Error::Shutdown(mpsc::RecvTimeoutError::Disconnected)))
    );
    assert!(server.requests.try_recv().is_err());
}
