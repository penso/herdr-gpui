#![allow(clippy::unwrap_used)]
use super::*;
use crate::cloud;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

fn sh(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    command
}

#[test]
fn the_session_has_a_terminal_of_80_columns_by_32_rows_on_every_stream() {
    let finished = create_vm(
        sh("test -t 0 && test -t 1 && test -t 2 && stty size"),
        &|| false,
    )
    .unwrap();
    assert_eq!(finished.code, Some(0));
    assert_eq!(finished.output, "32 80");
    assert_eq!((COLUMNS, ROWS), (80, 32));
}

#[test]
fn the_exit_status_is_reported_and_the_output_is_bounded_and_stripped_of_controls() {
    assert_eq!(create_vm(sh("exit 13"), &|| false).unwrap().code, Some(13));
    let finished = create_vm(sh(r#"printf '\033[31mred\033[0m\r\nplain\r\n'"#), &|| false).unwrap();
    assert_eq!(finished.output, "red\nplain");
    // Far more than a terminal buffer holds: unread output would block it.
    let finished = create_vm(
        sh("i=0; while [ $i -lt 4000 ]; do echo \"line $i padding padding padding\"; i=$((i+1)); done"),
        &|| false,
    )
    .unwrap();
    assert!(finished.output.contains("line 3999") && finished.output.len() < 8 * 1024);
}

#[test]
fn cancelling_kills_the_session_without_waiting_for_it() {
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let result = create_vm(sh("exec sleep 30"), &|| stop.load(Ordering::Acquire));
    canceller.join().unwrap();
    assert!(matches!(result, Err(Error::Cloud(cloud::Error::Cancelled))));
    assert!(started.elapsed() < Duration::from_secs(10));
}
