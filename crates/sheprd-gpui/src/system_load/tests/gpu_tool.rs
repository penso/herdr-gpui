//! Running a GPU tool on this machine: its answer, and the failures that
//! must not hold up the load worker.
use super::*;
use crate::system_load::gpu::{TOOL_LIMIT, tool};

fn sh(script: &str, timeout: Duration) -> Option<String> {
    tool("/bin/sh", ["-c", script], timeout)
}

#[test]
fn a_tool_that_exits_cleanly_is_read() {
    let limit = Duration::from_secs(10);
    assert_eq!(sh("echo 45", limit).as_deref(), Some("45\n"));
    assert_eq!(sh("echo 45; exit 1", limit), None, "a failure is no answer");
    assert_eq!(tool("herdr-no-such-gpu-tool", [], limit), None);
}

#[test]
fn a_tool_that_closes_its_output_and_keeps_running_is_ended() {
    let started = Instant::now();
    assert_eq!(sh("exec >&-; sleep 30", Duration::from_millis(300)), None);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_tool_that_keeps_printing_is_ended() {
    let started = Instant::now();
    assert_eq!(
        sh(
            "while :; do echo 1; sleep 0.05; done",
            Duration::from_millis(300)
        ),
        None
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn an_answer_past_the_cap_is_no_answer() {
    let limit = Duration::from_secs(10);
    let full = sh(&format!("head -c {TOOL_LIMIT} /dev/zero"), limit);
    assert_eq!(full.map(|text| text.len()), Some(TOOL_LIMIT));
    assert_eq!(
        sh(&format!("head -c {} /dev/zero", TOOL_LIMIT + 1), limit),
        None
    );
}
