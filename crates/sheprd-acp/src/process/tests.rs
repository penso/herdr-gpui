#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// The agent exits at once on TERM, but leaves a tool behind that ignores
/// it; stopping the chat must still end the tool.
#[test]
fn stopping_ends_a_tool_that_ignores_term_after_the_agent_exits() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("tool.pid");
    let script = format!(
        "(trap '' TERM; echo $$ > {path}; exec sleep 30) & \
         while [ ! -s {path} ]; do sleep 0.01; done; exec sleep 30",
        path = pid_file.display()
    );
    let spawned = spawn(
        &OsString::from("/bin/sh"),
        &[OsString::from("-c"), OsString::from(script)],
        &[],
        dir.path(),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let tool = loop {
        if let Ok(text) = std::fs::read_to_string(&pid_file)
            && let Ok(pid) = text.trim().parse::<i32>()
        {
            break rustix::process::Pid::from_raw(pid).unwrap();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the tool never started"
        );
        thread::sleep(POLL);
    };
    spawned.process.stop();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while rustix::process::test_kill_process(tool).is_ok() {
        assert!(
            std::time::Instant::now() < deadline,
            "the tool outlived its chat"
        );
        thread::sleep(POLL);
    }
}

#[cfg(unix)]
#[test]
fn the_agent_gets_the_environment_it_was_started_with() {
    use std::io::Read as _;

    let dir = tempfile::tempdir().unwrap();
    let mut spawned = spawn(
        &OsString::from("/bin/sh"),
        &[
            OsString::from("-c"),
            OsString::from("printf %s \"$HERDR_ACP_TEST\""),
        ],
        &[("HERDR_ACP_TEST", "1")],
        dir.path(),
    )
    .unwrap();
    let mut seen = String::new();
    spawned.output.read_to_string(&mut seen).unwrap();
    assert_eq!(seen, "1");
}
