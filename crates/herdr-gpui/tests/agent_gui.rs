//! Daemon-free native process test. No inherited discovery or agent preview state.
#![cfg(all(feature = "integration-test", target_os = "macos"))]
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
#[path = "../../test-support/sandbox.rs"]
mod sandbox;

use sandbox::{Sandbox, stop_children};
use std::{
    fs,
    process::Child,
    thread,
    time::{Duration, Instant},
};

struct Fixture {
    sandbox: Sandbox,
    child: Option<Child>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        stop_children([&mut self.child]);
        if let Ok(log) = fs::read_to_string(self.sandbox.dir.join("agent.log")) {
            eprintln!("{log}");
        }
    }
}

#[test]
#[ignore = "opens daemon-free native agent fixture; requires an active macOS desktop"]
fn native_agent_view() {
    let mut fixture = Fixture {
        sandbox: Sandbox::new(),
        child: None,
    };
    let mut command = fixture
        .sandbox
        .command(env!("CARGO_BIN_EXE_herdr-gpui"), "agent.log");
    command
        .arg("--agent-test")
        .env_remove("HERDR_AGENT_PREVIEW");
    for name in ["DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    fixture.child = Some(command.spawn().expect("launch native agent fixture"));
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = fixture.child.as_mut().unwrap().try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "native agent fixture timed out");
        thread::sleep(Duration::from_millis(50));
    };
    let log = fs::read_to_string(fixture.sandbox.dir.join("agent.log")).unwrap();
    assert!(
        status.success(),
        "native agent fixture failed: {status}\n{log}"
    );
    assert!(
        log.lines().any(|line| line == "AGENT native PASS"),
        "missing success marker:\n{log}"
    );
}
