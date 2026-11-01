#![allow(clippy::unwrap_used)]
use super::*;
use crate::teleport::{Host, carry::shell_review};
use core::prelude::v1::test;
use herdr_client::ConnectTarget;
use std::sync::Arc;

fn place(endpoint_id: &str) -> Place {
    Place {
        endpoint_id: endpoint_id.into(),
        label: endpoint_id.into(),
        host: Host::new(&ConnectTarget::Local).unwrap(),
    }
}

/// A carry at `stage` whose worker is already gone.
fn abandoned(stage: Stage) -> Carry {
    let (_, events) = mpsc::channel();
    Carry {
        origin: Origin {
            place: place(crate::endpoint::LOCAL),
            workspace_id: "w1".into(),
            pane_id: "inactive".into(),
        },
        destination: "There".into(),
        stage,
        cancelled: Arc::new(AtomicBool::new(false)),
        events,
    }
}

/// A worker that never started, or stopped without answering, fails the
/// review or move instead of leaving it waiting, and a move left that way
/// no longer blocks later ones.
#[test]
fn a_lost_worker_fails_the_review_or_move() {
    let mut carry = abandoned(Stage::Reviewing);
    assert!(matches!(carry.poll(), (true, None)));
    assert!(matches!(&carry.stage, Stage::Failed(error) if error.contains("stopped")));

    let mut carry = abandoned(Stage::Moving(Some(Step::Fetch)));
    assert!(matches!(
        carry.poll(),
        (true, Some(Err(crate::teleport::Error::WorkerStopped)))
    ));
    assert!(!carry.moving());

    // A review that already answered is not disturbed by its worker ending.
    let mut carry = abandoned(Stage::Ready(shell_review(place("ssh:there"))));
    assert!(matches!(carry.poll(), (false, None)));
    assert!(matches!(carry.stage, Stage::Ready(_)));
}

/// The agent may start working while the review waits for Enter; the move
/// then refuses rather than stopping it mid-task.
#[gpui::test]
fn an_agent_that_starts_working_during_the_review_is_not_moved(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut snapshot = crate::sidebar::layout_tests::snapshot(1);
        snapshot.agents.truncate(1);
        snapshot.agents[0].pane_id = "inactive".into();
        snapshot.agents[0].agent_status = AgentStatus::Working;
        view.live.snapshot = Some(Arc::new(snapshot));
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.pane_carry = Some(abandoned(Stage::Ready(shell_review(place("ssh:there")))));
            v.menu.page = Some(Page::CarryPane);
            v.submit_carry(window, cx);
            let carry = v.pane_carry.as_ref().unwrap();
            assert!(!carry.moving());
            assert!(
                matches!(&carry.stage, Stage::Failed(error) if error.contains("working")),
                "the move must not start"
            );
        });
    });
}

mod en_xa;
mod session_only;
