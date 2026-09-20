//! Requires an active native desktop and an explicitly selected daemon executable.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "../../test-support/sandbox.rs"]
mod sandbox;

use sandbox::{Sandbox, daemon_binary, stop_children};
use std::{
    fs,
    process::Child,
    thread,
    time::{Duration, Instant},
};

struct Isolated {
    sandbox: Sandbox,
    daemon: Option<Child>,
    gui: Option<Child>,
}

#[test]
#[ignore = "requires active native desktop; GUI-only fixtures, no daemon"]
fn native_sidebar() {
    let mut isolated = Isolated {
        sandbox: Sandbox::new(),
        daemon: None,
        gui: None,
    };
    let mut command = isolated
        .sandbox
        .command(env!("CARGO_BIN_EXE_herdr-gpui"), "gui.log");
    for name in ["DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    isolated.gui = Some(command.arg("--sidebar-test").spawn().unwrap());
    let gui = isolated.gui.as_mut().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while gui.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = gui.kill();
            let _ = gui.wait();
            panic!("native sidebar timeout");
        }
        thread::sleep(Duration::from_millis(50));
    }
    let status = gui.wait().unwrap();
    let log = fs::read_to_string(isolated.sandbox.dir.join("gui.log")).unwrap();
    eprintln!("{log}");
    assert!(status.success(), "native sidebar failed");
    assert!(log.contains("SIDEBAR native PASS:"));
}

impl Drop for Isolated {
    fn drop(&mut self) {
        stop_children([&mut self.gui, &mut self.daemon]);
        for name in ["gui.log", "daemon.log"] {
            if let Ok(log) = fs::read_to_string(self.sandbox.dir.join(name)) {
                eprintln!("{name} (last 40 lines, at most 600 chars each):");
                let lines = log.lines().rev().take(40).collect::<Vec<_>>();
                for line in lines.into_iter().rev() {
                    eprintln!("{}", line.chars().take(600).collect::<String>());
                }
            }
        }
    }
}

#[test]
#[ignore = "requires active desktop and explicit HERDR_TEST_BINARY; launches a native GUI and isolated daemon"]
fn native_gui_live() {
    let binary = daemon_binary();
    let mut isolated = Isolated {
        sandbox: Sandbox::new(),
        daemon: None,
        gui: None,
    };
    let socket = isolated.sandbox.socket();
    isolated.daemon = Some(
        isolated
            .sandbox
            .command(binary, "daemon.log")
            .arg("server")
            .spawn()
            .unwrap(),
    );
    isolated
        .sandbox
        .wait_for_daemon(isolated.daemon.as_mut().unwrap(), Duration::from_secs(20));
    let mut gui_command = isolated
        .sandbox
        .command(env!("CARGO_BIN_EXE_herdr-gpui"), "gui.log");
    // Only the GUI needs desktop transport; never inherit user config or discovery variables.
    for name in ["DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY"] {
        if let Some(value) = std::env::var_os(name) {
            gui_command.env(name, value);
        }
    }
    isolated.gui = Some(
        gui_command
            .arg("--socket")
            .arg(&socket)
            .arg("--integration-test")
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = isolated.gui.as_mut().unwrap().try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "native GUI timeout (active desktop required)"
        );
        assert!(
            isolated
                .daemon
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .is_none(),
            "daemon died while GUI was running"
        );
        thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "GUI failed: {status}");
    let log = fs::read_to_string(isolated.sandbox.dir.join("gui.log")).unwrap();
    assert!(
        log.contains("GUI integration PASS:"),
        "GUI exited without completing harness"
    );
    assert!(
        log.contains("GUI input pipeline verified:"),
        "GUI did not verify native action, key, and text delivery"
    );
    assert!(
        log.contains("GUI external workspace push verified:")
            && log.contains("bound_ms=3000 observation_poll_ms=100 unchanged_connection=true unchanged_focus=true no_refresh=true")
            && log.contains("3 workspaces / 4 tabs"),
        "GUI did not verify bounded external workspace delivery without refresh/reconnect"
    );
    assert!(
        isolated
            .daemon
            .as_mut()
            .unwrap()
            .try_wait()
            .unwrap()
            .is_none(),
        "GUI exit killed daemon"
    );
    eprintln!(
        "GUI exited successfully; isolated daemon is still alive; cleaning up only owned children"
    );
}
