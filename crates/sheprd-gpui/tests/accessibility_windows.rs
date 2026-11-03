//! Reads a running window through UI Automation, the interface Narrator and
//! NVDA use, and checks what it names and where F6 and Tab take the keyboard.
//! Requires an active Windows desktop and an explicitly selected daemon
//! executable (`HERDR_TEST_BINARY`), like the live GUI test.
#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
#[path = "../../test-support/sandbox.rs"]
mod sandbox;

use sandbox::{Sandbox, daemon_binary, stop_children};
use std::{
    fs,
    path::Path,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

struct Running {
    sandbox: Sandbox,
    daemon: Option<Child>,
    gui: Option<Child>,
}

impl Drop for Running {
    fn drop(&mut self) {
        stop_children([&mut self.gui, &mut self.daemon]);
    }
}

/// The tree, then the focus after each of `keys`, from `support/uia_tree.ps1`.
fn read(script: &Path, process: u32, keys: &str) -> String {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(script)
        .args(["-ProcessId", &process.to_string(), "-Keys", keys])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "UI Automation script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
#[ignore = "requires an active Windows desktop and HERDR_TEST_BINARY; launches a native GUI and isolated daemon"]
fn ui_automation_names_the_chrome_and_f6_reaches_it() {
    let binary = daemon_binary();
    let mut running = Running {
        sandbox: Sandbox::new(),
        daemon: None,
        gui: None,
    };
    let daemon = running
        .sandbox
        .command(binary, "daemon.log")
        .arg("server")
        .spawn();
    running.daemon = Some(daemon.unwrap());
    running
        .sandbox
        .wait_for_daemon(running.daemon.as_mut().unwrap(), Duration::from_secs(20));
    let socket = running.sandbox.socket();
    let gui = running
        .sandbox
        .command(env!("CARGO_BIN_EXE_sheprd"), "gui.log")
        .arg("--socket")
        .arg(&socket)
        .spawn();
    running.gui = Some(gui.unwrap());
    let process = running.gui.as_ref().unwrap().id();
    let script = running.sandbox.dir.join("uia_tree.ps1");
    fs::write(&script, include_str!("support/uia_tree.ps1")).unwrap();

    // AccessKit builds the tree once a client first asks, a frame later, and
    // the sidebar arrives with the first snapshot after that.
    let expected = [
        "MenuBar 'Menu bar'",
        "MenuItem 'File' [Collapsed]",
        "Group 'Sidebar'",
        "List 'Workspaces'",
        "ListItem '",
        "Button 'New workspace' [Invoke]",
        "List 'Agents'",
        "Tab 'Activity'",
        "Button 'Settings' [Invoke]",
        "Button 'Devices: ",
        "TabItem '1' [selected=True]",
        "Button 'New tab' [Invoke]",
        "Document 'Terminal'",
        "StatusBar 'Status bar'",
        "Button 'Theme' [Invoke]",
        "Button 'Version ",
    ];
    let deadline = Instant::now() + Duration::from_secs(60);
    let tree = loop {
        let tree = read(&script, process, "");
        let missing: Vec<_> = expected
            .iter()
            .filter(|name| !tree.contains(**name))
            .collect();
        if missing.is_empty() {
            break tree;
        }
        assert!(
            Instant::now() < deadline,
            "missing {missing:?} in:
{tree}"
        );
        thread::sleep(Duration::from_millis(500));
    };
    eprintln!("{tree}");
    // Back and Forward have no history to go to yet.
    assert!(tree.contains("Button 'Back' [disabled]"), "{tree}");

    let focus = read(&script, process, "{F6}|{TAB}|{F6}|{F6}|+{TAB}|{ESC}|+{F6}");
    eprintln!("{focus}");
    let moves: Vec<&str> = focus
        .lines()
        .filter_map(|line| line.split_once(" -> ").map(|(_, focused)| focused))
        .collect();
    assert_eq!(moves.len(), 7, "{focus}");
    // Title bar, then Tab skips the unavailable Back and Forward.
    assert!(moves[0].starts_with("Button 'Sidebar'"), "{focus}");
    assert!(
        !moves[1].contains("'Back'") && moves[1] != moves[0],
        "{focus}"
    );
    // The sidebar starts at its activity bar.
    assert!(moves[2].starts_with("TabItem 'Workspaces"), "{focus}");
    // The status bar, and Shift-Tab wraps within it.
    assert!(
        moves[3].starts_with("Button ") || moves[3].starts_with("Text "),
        "{focus}"
    );
    assert_ne!(moves[4], moves[3], "{focus}");
    // Escape and the terminal; Shift-F6 goes back to the last region.
    assert!(moves[5].starts_with("Document 'Terminal'"), "{focus}");
    assert!(!moves[6].starts_with("Document"), "{focus}");
}
