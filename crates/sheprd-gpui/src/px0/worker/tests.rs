#![allow(clippy::unwrap_used)]
use super::*;
use crate::px0::tests::HttpServer;

#[cfg(unix)]
mod fixture;
mod replacement;

#[test]
fn attached_workbench_is_never_owned_or_stopped() {
    let server = HttpServer::new(200, r#"{"workbench":true,"version":"1"}"#);
    let worker = Worker::start(
        Config {
            url: Some(server.url.clone()),
        },
        None,
    )
    .unwrap();
    server
        .requests
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    assert!(worker.owned.lock().unwrap().child.is_none());
    worker.shutdown();
    server::probe(&server.url).unwrap();
}

#[test]
fn replacing_an_attached_worker_keeps_the_external_server() {
    let first = HttpServer::new(200, r#"{"workbench":true,"version":"1"}"#);
    let second = HttpServer::new(200, r#"{"workbench":true,"version":"1"}"#);
    let worker = Worker::start(
        Config {
            url: Some(first.url.clone()),
        },
        None,
    )
    .unwrap();
    first.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let replacement = Worker::start(
        Config {
            url: Some(second.url.clone()),
        },
        Some(worker),
    )
    .unwrap();
    second
        .requests
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    replacement.shutdown();
    server::probe(&first.url).unwrap();
    server::probe(&second.url).unwrap();
}

#[test]
fn cancellation_prevents_start_even_with_a_ready_discovery_record() {
    let temp = tempfile::tempdir().unwrap();
    let (stop, stopped) = mpsc::sync_channel(1);
    stop.send(()).unwrap();
    let owned = Mutex::new(Owned::default());
    assert!(
        launch(
            Path::new("nonexistent-px0"),
            temp.path(),
            None,
            &stopped,
            &owned
        )
        .unwrap()
        .is_none()
    );
    assert!(owned.lock().unwrap().child.is_none());
}

#[cfg(unix)]
#[test]
fn owned_launch_uses_its_published_ephemeral_port_and_is_reaped() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let program = temp.path().join("fake-px0");
    // Re-exec the native test executable: Python launchers on macOS may
    // introduce another process, unlike the native px0 binary we supervise.
    let executable = std::env::current_exe().unwrap();
    let executable = sheprd_client::shell_quote(executable.to_str().unwrap());
    std::fs::write(
        &program,
        format!(
            r#"#!/bin/sh
printf '%s\n' "$@" > "$XDG_STATE_HOME/args.txt"
export HERDR_PX0_TEST_CHILD=1
exec {executable} --exact px0::worker::tests::fixture::workbench --nocapture
"#
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (_stop, stopped) = mpsc::sync_channel(1);
    let owned = Mutex::new(Owned::default());
    let result = launch(&program, temp.path(), None, &stopped, &owned);
    // Always reap our child before assertions can unwind or remove its folder.
    cleanup(&owned);
    let url = result.unwrap().unwrap();
    let found = server::discover(&temp.path().join("px0/workbench.lock"))
        .unwrap()
        .unwrap();
    assert_eq!(url, found.url);
    assert!(server::probe(&url).is_err());
    assert!(owned.lock().unwrap().child.is_none());
    let text = std::fs::read_to_string(temp.path().join("args.txt")).unwrap();
    let args: Vec<_> = text.lines().collect();
    assert!(args.windows(2).any(|pair| pair == ["-port", "0"]));
    assert!(args.windows(2).any(|pair| pair == ["-host", "127.0.0.1"]));
    assert!(args.contains(&"-no-open"));
    assert!(args.contains(&"-no-update"));
}
