use super::*;

mod context;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod local_feedback;

pub fn notification(title: &str) -> SemanticNotification {
    SemanticNotification {
        kind: SemanticNotificationKind::NeedsAttention,
        title: title.into(),
        body: Some("Review needed".into()),
        sound: Some(herdr_client::protocol::SemanticNotificationSound::Request),
        agent: Some("untrusted".into()),
        workspace_id: Some("duplicate-id".into()),
        tab_id: None,
        pane_id: None,
        position: Some(ToastHerdrPosition::TopLeft),
    }
}

#[test]
#[allow(clippy::unwrap_used)]
fn targets_validate_specificity_membership_and_boot_without_fallback() {
    let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    for (tab, pane, expected) in [
        (None, None, NavigationTarget::Workspace("w1")),
        (Some("w1:t1"), None, NavigationTarget::Tab("w1:t1")),
        (
            Some("w1:t1"),
            Some("w1:p1"),
            NavigationTarget::Pane("w1:p1"),
        ),
    ] {
        let mut wire = notification("target");
        wire.workspace_id = Some("w1".into());
        wire.tab_id = tab.map(str::to_owned);
        wire.pane_id = pane.map(str::to_owned);
        let notice = Notice::new(wire, Instant::now()).with_snapshot(Some(&snapshot));
        assert_eq!(notice.target(&snapshot), Some(expected));
        let mut stale = snapshot.clone();
        stale.boot_id = "new-boot".into();
        assert!(notice.target(&stale).is_none());
        stale = snapshot.clone();
        stale.workspaces.clear();
        assert!(notice.target(&stale).is_none());
        if tab.is_some() {
            stale = snapshot.clone();
            stale.tabs.clear();
            assert!(notice.target(&stale).is_none());
        }
        if pane.is_some() {
            stale = snapshot.clone();
            stale.panes.clear();
            assert!(notice.target(&stale).is_none());
        }
    }
    let mut wire = notification("pane only");
    wire.workspace_id = None;
    wire.pane_id = Some("w1:p1".into());
    let notice = Notice::new(wire.clone(), Instant::now()).with_snapshot(Some(&snapshot));
    assert_eq!(notice.workspace_id.as_deref(), Some("w1"));
    assert_eq!(notice.tab_id.as_deref(), Some("w1:t1"));
    let mut moved = snapshot.clone();
    moved.panes[0].tab_id = "other".into();
    assert!(notice.target(&moved).is_none());
    wire.tab_id = Some("wrong-parent".into());
    assert!(
        Notice::new(wire, Instant::now())
            .with_snapshot(Some(&snapshot))
            .target(&snapshot)
            .is_none()
    );
    let mut custom = notification("custom");
    custom.kind = SemanticNotificationKind::Custom;
    custom.workspace_id = None;
    assert!(
        Notice::new(custom, Instant::now())
            .with_snapshot(Some(&snapshot))
            .target(&snapshot)
            .is_none()
    );
}

#[test]
fn notifications_bound_untrusted_text_and_preserve_position() {
    let mut wire = notification(&"\u{1b}\n\u{202e}\u{2066}界".repeat(1000));
    wire.body = Some("界".repeat(1000));
    let notice = Notice::new(wire, Instant::now());
    assert!(notice.title.chars().count() <= 160);
    assert!(notice.title.chars().all(|c| c == '界'));
    assert_eq!(notice.body.as_ref().map(|b| b.chars().count()), Some(512));
    assert_eq!(notice.position, ToastHerdrPosition::TopLeft);
    assert_eq!(safe_text(&"\0".repeat(10000), 160), "");
    assert_eq!(
        Notice::new(notification("\n\u{202e}"), Instant::now()).title,
        "Notification"
    );
}

#[test]
fn ingress_is_bounded_and_same_pane_replaces_without_coalescing_targetless() {
    let now = Instant::now();
    let mut toasts = Toasts::default();
    toasts.receive((0..20).map(|id| Notice::new(notification(&id.to_string()), now)));
    assert_eq!(toasts.entries.len(), PENDING_LIMIT * 2 + 1);
    assert_eq!(toasts.entries[0].1.title, "3");
    toasts.dismiss(18);
    assert_eq!(toasts.entries.len(), 16);
    let mut wire = notification("pane");
    wire.pane_id = Some("p".into());
    toasts.receive([Notice::new(wire.clone(), now), Notice::new(wire, now)]);
    assert_eq!(toasts.entries.len(), 17);
}
