#![allow(clippy::unwrap_used)]
use super::*;
use sheprd_client::protocol::ClientShellTab;

fn snapshot() -> ClientShellSnapshot {
    serde_json::from_str(include_str!(
        "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap()
}

fn pane_notice(snapshot: &ClientShellSnapshot) -> Notice {
    let mut wire = notification("claude finished");
    wire.body = Some("10 · 1".into());
    wire.workspace_id = None;
    wire.pane_id = Some("w1:p1".into());
    Notice::new(wire, Instant::now()).with_snapshot(Some(snapshot))
}

#[test]
fn agent_notice_names_its_task_and_sidebar_place() {
    let snapshot = snapshot();
    // A linked worktree is named by its branch, as its sidebar row is.
    assert_eq!(pane_notice(&snapshot).body.as_deref(), Some("Review\nmain"));

    let mut named = snapshot.clone();
    named.workspaces[0].worktree = None;
    named.agents[0].title = None;
    named.agents[0].terminal_title_stripped = Some("Fix login flow".into());
    named.tabs.push(ClientShellTab {
        tab_id: "w1:t2".into(),
        number: 2,
        label: "logs".into(),
        ..named.tabs[0].clone()
    });
    assert_eq!(
        pane_notice(&named).body.as_deref(),
        Some("Fix login flow\nrepo · main")
    );
}

#[test]
fn agent_name_alone_is_not_a_task_and_unknown_panes_keep_the_daemon_body() {
    let mut snapshot = snapshot();
    snapshot.agents[0].title = Some(" claude ".into());
    snapshot.agents[0].terminal_title_stripped = None;
    assert_eq!(pane_notice(&snapshot).body.as_deref(), Some("main"));

    snapshot.agents[0].title = Some("\u{1b}]0;evil\u{7}\u{202e}Task".into());
    assert_eq!(
        pane_notice(&snapshot).body.as_deref(),
        Some("]0;evilTask\nmain")
    );

    // An unresolved target never borrows another pane's context.
    let mut wire = notification("claude finished");
    wire.body = Some("10 · 1".into());
    wire.pane_id = Some("gone".into());
    let notice = Notice::new(wire, Instant::now()).with_snapshot(Some(&snapshot));
    assert_eq!(notice.body.as_deref(), Some("10 · 1"));

    // Local feedback and previews keep their own text.
    let mut wire = notification("preview");
    wire.workspace_id = None;
    wire.pane_id = Some("w1:p1".into());
    let preview = Notice::new(wire, Instant::now())
        .preview()
        .with_snapshot(Some(&snapshot));
    assert_eq!(preview.body.as_deref(), Some("Review needed"));
}

#[test]
fn custom_notices_keep_the_senders_text() {
    let snapshot = snapshot();
    let mut wire = notification("deploy");
    wire.kind = SemanticNotificationKind::Custom;
    wire.body = Some("Staging is green".into());
    wire.workspace_id = None;
    wire.pane_id = Some("w1:p1".into());
    let notice = Notice::new(wire, Instant::now()).with_snapshot(Some(&snapshot));
    assert!(notice.target(&snapshot).is_some());
    assert_eq!(notice.body.as_deref(), Some("Staging is green"));
    assert!(!notice.described);
    assert!(pane_notice(&snapshot).described);
}
