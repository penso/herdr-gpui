use super::*;
use crate::teleport::carry::SessionReview;

#[gpui::test]
fn a_session_only_move_rechecks_busy_status_on_confirm(cx: &mut TestAppContext) {
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
            let review = Review::SessionOnly(Box::new(SessionReview::fixture(place("ssh:there"))));
            v.pane_carry = Some(abandoned(Stage::Ready(review)));
            v.menu.page = Some(Page::CarryPane);
            v.submit_carry(window, cx);
            let carry = v.pane_carry.as_ref().unwrap();
            assert!(!carry.moving());
            assert!(matches!(&carry.stage, Stage::Failed(error) if error.contains("working")));
        });
    });
}
