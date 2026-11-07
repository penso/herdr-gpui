use super::*;
use std::cell::{Cell, RefCell};

mod line;
mod refusals;
mod stopping;

/// A Claude session as Herdr's own integration reports it.
const SESSION: &str = "0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10";

fn pane(value: serde_json::Value) -> teleport::snapshot::Pane {
    let mut pane = serde_json::json!({
        "pane_id": "w1:p1",
        "tab_id": "w1:t1",
        "cwd": "/shell",
        "foreground_cwd": "/foreground",
        "agent": "claude",
        "agent_session": {
            "agent": "claude",
            "kind": "id",
            "source": "herdr:claude",
            "value": SESSION,
        },
    });
    if let (Some(pane), Some(changes)) = (pane.as_object_mut(), value.as_object()) {
        pane.extend(changes.clone());
    }
    serde_json::from_value(pane).unwrap()
}

fn info(argv: &[&str], cwd: Option<&str>) -> ProcessInfo {
    serde_json::from_value(serde_json::json!({
        "foreground_process_group_id": 20,
        "shell_pid": 10,
        "foreground_processes": [{"pid": 20, "argv": argv, "cwd": cwd}],
    }))
    .unwrap()
}
