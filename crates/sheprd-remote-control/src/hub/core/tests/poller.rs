use super::*;
use crate::hub::core::poller::Poller;
use sheprd_core_client::{Error as CoreError, Method, wire::Doc};
use std::time::{Duration, SystemTime};

fn doc(version: u64, name: &str) -> sheprd_core_client::Result<Doc> {
    Ok(Doc {
        value: json!({"spaces": [{"id": "s", "folder": "/f", "name": name, "tabs": []}]}),
        version,
    })
}

fn labels(poller: &mut Poller) -> Vec<String> {
    poller
        .project("fake-core", &[], None, None)
        .workspaces
        .into_iter()
        .map(|workspace| workspace.label)
        .collect()
}

#[test]
fn a_slow_answer_keeps_the_spaces_and_only_a_refusal_clears_them() {
    let mut poller = Poller::default();
    poller.read_spaces(doc(1, "desk")).unwrap();
    assert_eq!(labels(&mut poller), ["desk"]);
    for error in [CoreError::Timeout(Method::DocGet), CoreError::Busy] {
        poller.read_spaces(Err(error)).unwrap();
        assert_eq!(labels(&mut poller), ["desk"]);
    }
    poller
        .read_spaces(Err(CoreError::Refused {
            kind: None,
            message: "no".into(),
            method: Method::DocGet,
        }))
        .unwrap();
    assert!(labels(&mut poller).is_empty());
}

#[test]
fn a_lost_connection_is_not_mistaken_for_a_slow_answer() {
    let error = Poller::default().read_spaces(Err(CoreError::Lost));
    assert!(matches!(
        error,
        Err(HubError::Core(Error::Client(CoreError::Lost)))
    ));
}

#[test]
fn the_document_is_parsed_again_only_when_its_version_changes() {
    let mut poller = Poller::default();
    poller.read_spaces(doc(1, "first")).unwrap();
    poller.read_spaces(doc(1, "same version")).unwrap();
    assert_eq!(labels(&mut poller), ["first"]);
    poller.read_spaces(doc(2, "second")).unwrap();
    assert_eq!(labels(&mut poller), ["second"]);
}

fn write_tab(path: &std::path::Path, workspace: &str, at: SystemTime) {
    let tabs = json!({"tabs": [{"id": 1, "scope": "s", "workspace_id": workspace, "title": "x",
        "location": {"kind": "core_terminal", "terminal": "t1"}}]});
    std::fs::write(path, tabs.to_string()).unwrap();
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

#[test]
fn saved_tabs_are_read_again_only_when_the_file_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("browser-tabs.json");
    let records = [serde_json::from_value(record("t1")).unwrap()];
    let mut poller = Poller::default();
    let project = |poller: &mut Poller, workspace: &str| {
        let layout = poller.project("fake-core", &records, Some(&path), Some("s"));
        layout.workspace(workspace).is_some()
    };
    let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    write_tab(&path, "first", at);
    assert!(project(&mut poller, "first"));
    write_tab(&path, "second", at);
    assert!(project(&mut poller, "first"));
    write_tab(&path, "second", at + Duration::from_secs(1));
    assert!(project(&mut poller, "second"));
}

#[test]
fn a_saved_workspace_herdr_one_no_longer_lists_shows_its_folder() {
    let saved = serde_json::from_value(json!({"tabs": [
        {"id": 4, "scope": "s", "workspace_id": "w-opaque", "title": "x",
         "location": {"kind": "core_terminal", "terminal": "t1"}}
    ]}))
    .unwrap();
    let mut folderless = record("t1");
    folderless["cwd"] = json!("");
    for (record, label) in [(record("t1"), "app"), (folderless, "Herdr 2")] {
        let records = [serde_json::from_value(record).unwrap()];
        let layout = projection::project("fake-core", &records, Some(&saved), Some("s"));
        assert_eq!(layout.workspace("w-opaque").unwrap().label, label);
    }
}
