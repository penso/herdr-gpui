use super::*;
use crate::hub::core::poller::Poller;
use sheprd_core_client::wire::Doc;
use std::path::Path;

const SCOPE: &str = "local:/test.sock";

fn save_split(path: &Path) {
    let tabs = json!({"tabs": [
        {"id": 4, "scope": SCOPE, "workspace_id": "w1", "title": "build",
         "location": {"kind": "core_terminal", "panes": {"axis": "row", "first": "t2", "second": "t3"}}}
    ]});
    std::fs::write(path, tabs.to_string()).unwrap();
}

fn space(id: &str, name: &str, machine: Option<&str>, terminal: &str) -> Value {
    json!({"id": id, "folder": "/f", "name": name, "machine": machine, "tabs": [
        {"id": format!("{id}-tab"), "name": "one",
         "root": {"type": "block", "content": {"terminal": terminal}}}
    ]})
}

fn desk(terminal: &str) -> Poller {
    spaces(vec![space("s", "desk", None, terminal)])
}

fn spaces(spaces: Vec<Value>) -> Poller {
    let mut poller = Poller::default();
    let value = json!({ "spaces": spaces });
    poller
        .read_spaces("fake-core", Ok(Doc { value, version: 1 }))
        .unwrap();
    poller
}

fn panes(terminal: &crate::hub::Terminal) -> Vec<String> {
    terminal
        .panes
        .iter()
        .map(|pane| pane.pane.clone())
        .collect()
}

#[test]
fn a_saved_split_survives_a_desktop_document() {
    let dir = tempfile::tempdir().unwrap();
    let tabs = dir.path().join("browser-tabs.json");
    save_split(&tabs);
    let records =
        ["t1", "t2", "t3"].map(|address| serde_json::from_value(record(address)).unwrap());

    let layout = desk("t1").project("fake-core", &records, Some(&tabs), Some(SCOPE));

    let space = layout
        .workspaces
        .iter()
        .find(|row| row.label == "desk")
        .unwrap();
    assert_eq!(panes(&space.terminals[0]), [id("t1")]);
    let saved = &layout.workspace("w1").unwrap().terminals[0];
    assert_eq!(saved.label, "build");
    assert_eq!(panes(saved), [id("t2"), id("t3")]);
    assert_eq!(layout.workspaces.len(), 2, "nothing is listed twice");
}

#[test]
fn a_saved_tab_keeps_a_terminal_the_document_also_holds() {
    let dir = tempfile::tempdir().unwrap();
    let tabs = dir.path().join("browser-tabs.json");
    save_split(&tabs);
    let records = ["t2", "t3"].map(|address| serde_json::from_value(record(address)).unwrap());

    let layout = desk("t2").project("fake-core", &records, Some(&tabs), Some(SCOPE));

    let space = layout
        .workspaces
        .iter()
        .find(|row| row.label == "desk")
        .unwrap();
    assert!(space.terminals.is_empty());
    assert_eq!(
        panes(&layout.workspace("w1").unwrap().terminals[0]),
        [id("t2"), id("t3")]
    );
}

#[test]
fn another_sessions_tabs_leave_the_document_alone() {
    let dir = tempfile::tempdir().unwrap();
    let tabs = dir.path().join("browser-tabs.json");
    save_split(&tabs);
    let records = ["t2", "t3"].map(|address| serde_json::from_value(record(address)).unwrap());

    let layout = desk("t2").project("fake-core", &records, Some(&tabs), Some("local:/other"));

    let space = layout
        .workspaces
        .iter()
        .find(|row| row.label == "desk")
        .unwrap();
    assert_eq!(panes(&space.terminals[0]), [id("t2")]);
    assert!(layout.workspace("w1").is_none());
}

#[test]
fn a_local_space_keeps_a_saved_pane_another_machine_also_names() {
    let dir = tempfile::tempdir().unwrap();
    let tabs = dir.path().join("browser-tabs.json");
    save_split(&tabs);
    let records = ["t2", "t3"].map(|address| serde_json::from_value(record(address)).unwrap());
    let mut poller = spaces(vec![
        space("s", "desk", None, "t2"),
        space("far", "far", Some("box"), "t2"),
    ]);

    let layout = poller.project("fake-core", &records, Some(&tabs), Some(SCOPE));

    assert_eq!(
        panes(&layout.workspace("w1").unwrap().terminals[0]),
        [id("t2"), id("t3")]
    );
    assert_eq!(layout.working, 2);
}

#[test]
fn a_saved_tab_does_not_reveal_a_pane_only_another_machine_names() {
    let dir = tempfile::tempdir().unwrap();
    let tabs = dir.path().join("browser-tabs.json");
    save_split(&tabs);
    let records = ["t2", "t3"].map(|address| serde_json::from_value(record(address)).unwrap());
    let mut poller = spaces(vec![space("far", "far", Some("box"), "t2")]);

    let layout = poller.project("fake-core", &records, Some(&tabs), Some(SCOPE));

    assert_eq!(
        panes(&layout.workspace("w1").unwrap().terminals[0]),
        [id("t3")]
    );
    assert_eq!(layout.working, 1);
}
