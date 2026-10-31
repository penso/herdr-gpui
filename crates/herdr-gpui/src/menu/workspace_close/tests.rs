use super::*;

#[test]
fn pending_and_risky_reports_cannot_close_without_consent() {
    let snapshot = crate::sidebar::layout_tests::snapshot(7);
    let target = WorkspaceTarget::new(&snapshot, &snapshot.workspaces[3]);
    let mut check = CloseCheck::fixture(&snapshot, &target, None);
    assert!(!check.ready("close"));
    for report in [
        Report {
            dirty: true,
            ..Report::default()
        },
        Report {
            unpushed: true,
            ..Report::default()
        },
        Report {
            unknown: true,
            ..Report::default()
        },
    ] {
        check.report = Some(report);
        for text in ["", "Close", "yes", "clos"] {
            assert!(!check.ready(text));
        }
        assert!(check.ready("close"));
    }
    check.report = Some(Report::default());
    assert!(check.ready(""));
}

#[test]
fn changed_sibling_checkout_invalidates_group_consent() {
    let mut snapshot = crate::sidebar::layout_tests::snapshot(7);
    let target = WorkspaceTarget::new(&snapshot, &snapshot.workspaces[3]);
    let check = CloseCheck::fixture(&snapshot, &target, Some(Report::default()));
    assert!(check.current(&snapshot, &target));
    snapshot.workspaces[4].branch = Some("replacement".into());
    assert!(!check.current(&snapshot, &target));
}

#[test]
fn disconnected_worker_requires_consent_and_drop_cancels() {
    let snapshot = crate::sidebar::layout_tests::snapshot(7);
    let target = WorkspaceTarget::new(&snapshot, &snapshot.workspaces[3]);
    let mut check = CloseCheck::fixture(&snapshot, &target, None);
    assert!(check.poll());
    assert!(!check.ready(""));
    assert!(check.ready("close"));
    let cancelled = check.cancelled.clone();
    drop(check);
    assert!(cancelled.load(Ordering::Relaxed));
}
