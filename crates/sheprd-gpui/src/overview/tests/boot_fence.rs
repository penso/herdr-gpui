use super::{
    closed_page::close_working,
    page::{click, draw, fixture},
};
use crate::{sidebar::ActivityPanel, state::ConnectionStatus};
use std::sync::Arc;

#[gpui::test]
fn a_new_boot_cannot_reuse_a_closed_agents_workspace(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let (_, id) = close_working(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut shown = view.live.snapshot.as_deref().unwrap().clone();
            shown.boot_id = "another-session".into();
            // The new session reuses every workspace ID, but owns none of the
            // closed row's targets.
            view.live.snapshot = Some(Arc::new(shown));
            view.poll_overview(cx);
            assert!(view.overview.closed.get(id).is_some());
        })
    });
    draw(cx);
    click(cx, "overview-closed");
    click(cx, format!("overview-closed-{id}"));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.activity.panel, ActivityPanel::Overview);
        assert!(view.pending_navigation.is_none());
        assert!(
            view.flash
                .as_ref()
                .unwrap()
                .0
                .text
                .contains("original session")
        );
    });
    click(cx, format!("overview-reopen-{id}"));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(!view.overview_reopening());
        assert!(view.overview.closed.get(id).is_some());
        assert!(view.pending_navigation.is_none());
        assert!(
            view.flash
                .as_ref()
                .unwrap()
                .0
                .text
                .contains(&crate::Error::StaleConnection.to_string())
        );
    });
}

#[gpui::test]
fn a_reopen_waiting_for_connection_keeps_its_original_boot(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let (_, id) = close_working(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.live.status = ConnectionStatus::Disconnected;
            view.reopen_closed_agent(id, cx);
            assert!(view.overview_reopening());

            let mut shown = view.live.snapshot.as_deref().unwrap().clone();
            shown.boot_id = "restarted-while-waiting".into();
            view.live.snapshot = Some(Arc::new(shown));
            view.live.status = ConnectionStatus::Connected;
            view.poll_overview(cx);
            assert!(!view.overview_reopening());
            assert!(view.overview.closed.get(id).is_some());
            assert!(
                view.flash
                    .as_ref()
                    .unwrap()
                    .0
                    .text
                    .contains(&crate::Error::StaleConnection.to_string())
            );
        })
    });
}

#[gpui::test]
fn another_sessions_namesake_does_not_remove_the_closed_agent(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let original = cx.update(|_, cx| view.read(cx).live.snapshot.clone().unwrap());
    let (_, id) = close_working(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut shown = original.as_ref().clone();
            shown.boot_id = "another-session".into();
            view.live.snapshot = Some(Arc::new(shown));
            view.poll_overview(cx);
            assert_eq!(view.overview.closed.get(id).unwrap().boot, original.boot_id);

            // Returning to the original session really does bring it back.
            view.live.snapshot = Some(original);
            view.poll_overview(cx);
            assert!(view.overview.closed.get(id).is_none());
        })
    });
}
