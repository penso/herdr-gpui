#![allow(clippy::unwrap_used)]
use super::*;
use crate::browser::WebUrl;
use sheprd_client::protocol::AgentStatus;

fn web(value: &str) -> Option<Location> {
    Some(Location::Web {
        url: WebUrl::try_from(value).unwrap(),
    })
}

fn herdr(tab: &str, workspace: &str) -> ClosedHerdrTab {
    ClosedHerdrTab {
        tab_id: tab.into(),
        workspace_id: workspace.into(),
        label: None,
        cwd: None,
    }
}

fn snapshot() -> ClientShellSnapshot {
    serde_json::from_str(include_str!(
        "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap()
}

fn tab(id: &str, label: &str, custom: bool) -> ClientShellTab {
    ClientShellTab {
        tab_id: id.into(),
        workspace_id: "w".into(),
        number: 1,
        label: label.into(),
        custom_label: custom,
        zoomed: false,
        focused: false,
        agent_status: AgentStatus::Idle,
    }
}

fn pane(
    id: &str,
    tab: &str,
    cwd: Option<&str>,
    fg: Option<&str>,
    focused: bool,
) -> ClientShellPane {
    ClientShellPane {
        pane_id: id.into(),
        workspace_id: "w".into(),
        tab_id: tab.into(),
        label: None,
        cwd: cwd.map(Into::into),
        foreground_cwd: fg.map(Into::into),
        focused,
        right_click_passthrough: false,
    }
}

#[test]
fn closed_pages_and_terminals_reopen_last_closed_first() {
    let mut store = Store::default();
    let scope = Scope::endpoint("local");
    let page = store
        .open(scope.clone(), "w", web("https://example.com/a"), None)
        .unwrap();
    store.visited(page, None, Some("Example A"));
    store.close(page);
    store.record_closed_herdr_tab(scope.clone(), herdr("t1", "w"));

    assert!(matches!(
        store.pop_closed(&scope, "w"),
        Some(ClosedTab::Herdr(tab)) if tab.tab_id == "t1"
    ));
    let Some(ClosedTab::Page(closed)) = store.pop_closed(&scope, "w") else {
        panic!("expected the closed page");
    };
    assert_eq!(closed.title, "Example A");
    let reopened = store.reopen_page(closed).unwrap();
    assert_ne!(reopened, page);
    let tab = store.get(reopened).unwrap();
    assert_eq!(tab.title, "Example A");
    assert_eq!(tab.location, web("https://example.com/a"));
    assert!(store.pop_closed(&scope, "w").is_none());
}

#[test]
fn closed_tabs_belong_to_their_workspace_and_go_with_it() {
    let mut store = Store::default();
    let scope = Scope::endpoint("local");
    let other = Scope::endpoint("ssh:box");
    store.record_closed_herdr_tab(scope.clone(), herdr("t1", "w1"));
    store.record_closed_herdr_tab(other.clone(), herdr("t2", "w1"));
    // The same tab seen closed twice, by the dialog and the snapshot, is one entry.
    store.record_closed_herdr_tab(scope.clone(), herdr("t1", "w1"));
    assert!(store.peek_closed(&scope, "w2").is_none());
    assert!(store.has_workspaces(&scope, &["w1".into()]));
    store.forget_workspaces(&scope, &["w1".into()]);
    assert!(store.peek_closed(&scope, "w1").is_none());
    assert!(matches!(
        store.peek_closed(&other, "w1"),
        Some(ClosedTab::Herdr(tab)) if tab.tab_id == "t2"
    ));
}

#[test]
fn closed_history_is_bounded() {
    let mut store = Store::default();
    let scope = Scope::endpoint("local");
    for index in 0..MAX_CLOSED_TABS + 5 {
        store.record_closed_herdr_tab(scope.clone(), herdr(&format!("t{index}"), "w"));
    }
    let mut count = 0;
    while store.pop_closed(&scope, "w").is_some() {
        count += 1;
    }
    assert_eq!(count, MAX_CLOSED_TABS);
    for index in 0..MAX_CLOSED_WORKSPACES + 3 {
        store.record_closed_herdr_tab(scope.clone(), herdr("t", &format!("w{index}")));
    }
    assert!(store.peek_closed(&scope, "w0").is_none());
    assert!(
        store
            .peek_closed(&scope, &format!("w{}", MAX_CLOSED_WORKSPACES + 2))
            .is_some()
    );
}

#[test]
fn detaching_a_tab_is_not_a_close() {
    let mut store = Store::default();
    let scope = Scope::endpoint("local");
    let id = store.open(scope.clone(), "w", None, None).unwrap();
    assert!(store.detach(id).is_some());
    assert!(store.peek_closed(&scope, "w").is_none());
}

#[test]
fn a_singleton_tab_reopens_as_the_one_already_open() {
    let mut store = Store::default();
    let scope = Scope::endpoint("local");
    let first = store
        .open(scope.clone(), "w", Some(Location::Devices), None)
        .unwrap();
    store.close(first);
    let again = store
        .open(scope.clone(), "w", Some(Location::Devices), None)
        .unwrap();
    let Some(ClosedTab::Page(closed)) = store.pop_closed(&scope, "w") else {
        panic!("expected the closed overview");
    };
    assert_eq!(store.reopen_page(closed), Some(again));
    assert_eq!(store.in_workspace(&scope, "w").count(), 1);
}

#[test]
fn a_terminal_tab_reopens_with_its_label_and_folder() {
    let mut snapshot = snapshot();
    snapshot.tabs = vec![tab("t1", "build", true), tab("t2", "2", false)];
    snapshot.panes = vec![
        pane("p1", "t1", Some("/repo"), None, false),
        pane("p2", "t1", Some("/repo"), Some("/repo/web"), true),
        pane("p3", "t2", Some("/home"), None, false),
    ];
    let named = ClosedHerdrTab::capture(&snapshot, "t1").unwrap();
    assert_eq!(
        named.create_params(),
        json!({"workspace_id": "w", "focus": true, "cwd": "/repo/web", "label": "build"})
    );
    // A tab Herdr numbered itself gets a new number, not the old one.
    let numbered = ClosedHerdrTab::capture(&snapshot, "t2").unwrap();
    assert_eq!(
        numbered.create_params(),
        json!({"workspace_id": "w", "focus": true, "cwd": "/home"})
    );
    assert!(ClosedHerdrTab::capture(&snapshot, "gone").is_none());
}

#[test]
fn reopened_terminal_is_not_re_recorded_by_another_window_observation() {
    let mut store = Store::default();
    let scope = Scope::endpoint("local");
    // Window A records terminal t1 closed.
    store.record_closed_herdr_tab(scope.clone(), herdr("t1", "w"));

    // Window A reopens t1.
    assert!(matches!(
        store.pop_closed(&scope, "w"),
        Some(ClosedTab::Herdr(tab)) if tab.tab_id == "t1"
    ));
    assert!(store.peek_closed(&scope, "w").is_none());

    // Window B sees t1 disappeared in its snapshot and tries to record it.
    store.record_closed_herdr_tab(scope.clone(), herdr("t1", "w"));

    // t1 must not be added again!
    assert!(store.peek_closed(&scope, "w").is_none());
}

#[test]
fn reopened_page_restores_latest_visited_location() {
    let mut store = Store::default();
    let scope = Scope::endpoint("local");
    let page = store
        .open(scope.clone(), "w", web("https://example.com/first"), None)
        .unwrap();
    store.visited(page, None, Some("First Page"));
    // Navigate to a second page before closing.
    store.visited(page, web("https://example.com/second"), Some("Second Page"));
    store.close(page);

    let Some(ClosedTab::Page(closed)) = store.pop_closed(&scope, "w") else {
        panic!("expected closed page");
    };
    let reopened = store.reopen_page(closed).unwrap();
    let tab = store.get(reopened).unwrap();
    assert_eq!(tab.title, "Second Page");
    assert_eq!(tab.location, web("https://example.com/second"));
}

#[test]
fn a_terminal_whose_reopen_failed_can_be_reopened_again() {
    let mut store = Store::default();
    let scope = Scope::endpoint("local");
    store.record_closed_herdr_tab(scope.clone(), herdr("t1", "w"));
    let Some(ClosedTab::Herdr(tab)) = store.pop_closed(&scope, "w") else {
        panic!("expected the closed terminal");
    };

    // The tab.create request could not be queued, so the entry goes back.
    store.restore_closed_herdr_tab(scope.clone(), tab);
    assert!(matches!(
        store.peek_closed(&scope, "w"),
        Some(ClosedTab::Herdr(tab)) if tab.tab_id == "t1"
    ));

    // Another window observing the same close still does not duplicate it.
    store.record_closed_herdr_tab(scope.clone(), herdr("t1", "w"));
    assert!(store.pop_closed(&scope, "w").is_some());
    assert!(store.pop_closed(&scope, "w").is_none());
}
