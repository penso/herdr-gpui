use super::*;
use crate::sidebar::layout_tests::fixture_window;

/// A connected fixture window whose `w4` is an idle linked worktree with a
/// merged pull request in the cache.
fn merged_window(view: &mut SheprdWindow) -> Candidate {
    view.live.status = crate::state::ConnectionStatus::Connected;
    let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
    for workspace in &mut snapshot.workspaces {
        workspace.agent_status = AgentStatus::Idle;
    }
    let candidate = candidate(&snapshot, 4);
    view.live.snapshot = Some(Arc::new(snapshot));
    view.menu.pr_cache.seed(
        candidate.input.clone(),
        merged(State::Merged),
        Instant::now(),
    );
    candidate
}

/// A finished check of `candidate` that found it clean at the merged head.
fn checked(candidate: &Candidate) -> Probe {
    let (sender, receiver) = mpsc::sync_channel(1);
    sender.send(Ok(clean())).unwrap();
    Probe {
        candidate: candidate.clone(),
        receiver,
        cancelled: Arc::new(AtomicBool::new(false)),
    }
}

#[gpui::test]
fn pending_navigation_defers_the_decision(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let candidate = merged_window(view);
            let boot = view.live.snapshot.as_ref().unwrap().boot_id.clone();
            // The user just selected `w4`; the snapshot does not show it yet.
            view.live.activation = Some(crate::state::SurfaceActivation {
                request: "focus".into(),
                boot,
                revision: None,
                failed: false,
                focus: Some(crate::NavigationTarget::Workspace("w4".into())),
                active: true,
            });
            view.merged_cleanup.probe = Some(checked(&candidate));
            view.finish_merged_probe(window, cx);
            let now = Instant::now();
            assert!(view.merged_cleanup.removing.is_none());
            assert!(!view.merged_cleanup.ledger.waiting(&candidate, now));
            assert!(view.merged_cleanup.next_scan.is_some_and(|next| next > now));

            // Settled navigation decides. This fixture cannot send, so the
            // candidate waits for a retry rather than being dropped.
            view.live.activation = None;
            view.merged_cleanup.probe = Some(checked(&candidate));
            view.finish_merged_probe(window, cx);
            assert!(view.merged_cleanup.removing.is_none());
            assert!(
                view.merged_cleanup
                    .ledger
                    .waiting(&candidate, Instant::now())
            );
            assert!(
                !view
                    .merged_cleanup
                    .ledger
                    .waiting(&candidate, Instant::now() + RETRY)
            );
        })
    });
}

#[gpui::test]
fn a_refused_removal_is_reported_and_retried_without_force(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let candidate = merged_window(view);
            view.merged_cleanup.removing = Some((candidate.clone(), "auto".into()));
            // Another request's answer is not the cleanup's.
            view.live.cleanup_response = Some(("other".into(), Some(Ok(serde_json::json!({})))));
            view.finish_merged_removal(cx);
            assert!(view.merged_cleanup.removing.is_some());
            view.live.cleanup_response = Some((
                "auto".into(),
                Some(Ok(serde_json::json!({"error": {
                    "code": "dirty_worktree_requires_force",
                    "message": "modified or untracked files"
                }}))),
            ));
            view.finish_merged_removal(cx);
            assert!(view.merged_cleanup.removing.is_none());
            assert!(view.live.cleanup_response.is_none());
            assert!(
                view.local_error
                    .as_deref()
                    .is_some_and(|error| error.contains("dirty_worktree_requires_force"))
            );
            // No force removal is armed for a later dialog, and the
            // candidate is checked again once its retry is due.
            assert!(view.removal.is_none() && view.flash.is_none());
            let now = Instant::now();
            assert!(view.merged_cleanup.ledger.waiting(&candidate, now));
            assert!(!view.merged_cleanup.ledger.waiting(&candidate, now + RETRY));
        })
    });
}

#[gpui::test]
fn a_completed_removal_is_announced_and_settled(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let candidate = merged_window(view);
            view.merged_cleanup.removing = Some((candidate.clone(), "auto".into()));
            view.live.cleanup_response = Some((
                "auto".into(),
                Some(Ok(serde_json::json!({"result": {
                    "type": "worktree_removed", "workspace_id": "w4"
                }}))),
            ));
            view.finish_merged_removal(cx);
            assert!(view.merged_cleanup.removing.is_none() && view.local_error.is_none());
            assert_eq!(
                view.flash.as_ref().map(|(flash, _)| flash),
                Some(&crate::window::Flash::success(
                    "Removed the worktree of merged pull request #8"
                ))
            );
            assert!(
                view.merged_cleanup
                    .ledger
                    .waiting(&candidate, Instant::now() + RETRY * 100)
            );
        })
    });
}
