//! The tick rebuilds the rows only when they could have moved. A skipped
//! rebuild shows as a planted count surviving the poll.

use super::{
    at,
    closed_page::close_working,
    key,
    page::{fixture, window},
};
use crate::{
    SheprdWindow,
    notification_mutes::{Mutes, Scope},
    overview::{Marks, State},
    state::ConnectionStatus,
};
use gpui::{Entity, VisualTestContext};
use sheprd_client::protocol::AgentStatus;
use std::sync::Arc;

const PLANTED: usize = 99;

/// A window polled at `at(1000)`, with a count no rebuild would give.
fn settled(cx: &mut gpui::TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = fixture(cx);
    poll(&view, cx, 1000);
    cx.update(|_, cx| view.update(cx, |view, _| view.overview.drawn.2 = PLANTED));
    (view, cx)
}

fn poll(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, seconds: u64) {
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_overview_at(at(seconds), cx)));
}

fn rebuilt(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| view.read(cx).overview_needing()) != PLANTED
}

#[gpui::test]
fn nothing_changed_rebuilds_nothing_until_the_next_second(cx: &mut gpui::TestAppContext) {
    let (view, cx) = settled(cx);
    poll(&view, cx, 1000);
    assert!(!rebuilt(&view, cx));
    poll(&view, cx, 1001);
    assert!(rebuilt(&view, cx));
}

#[gpui::test]
fn a_rebuild_that_changes_marks_or_closed_agents_is_not_repeated(cx: &mut gpui::TestAppContext) {
    let (view, cx) = settled(cx);
    let revision = cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut shown = view.live.snapshot.as_deref().unwrap().clone();
            shown.agents.retain(|agent| agent.pane_id != "p1");
            view.live.snapshot = Some(Arc::new(shown));
            let revision = view.overview.closed.revision();
            view.poll_overview_at(at(1000), cx);
            revision
        })
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_ne!(view.overview.closed.revision(), revision);
        assert!(view.overview_inputs(at(1000), cx) == view.overview.inputs);
    });
}

#[gpui::test]
fn a_new_snapshot_rebuilds_within_the_second(cx: &mut gpui::TestAppContext) {
    let (view, cx) = settled(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut shown = view.live.snapshot.as_deref().unwrap().clone();
            shown.revision += 1;
            for agent in &mut shown.agents {
                agent.agent_status = AgentStatus::Blocked;
            }
            view.live.snapshot = Some(Arc::new(shown));
        })
    });
    poll(&view, cx, 1000);
    assert_eq!(cx.update(|_, cx| view.read(cx).overview_needing()), 3);
}

#[gpui::test]
fn losing_the_host_rebuilds_within_the_second(cx: &mut gpui::TestAppContext) {
    let (view, cx) = settled(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.live.status = ConnectionStatus::Disconnected;
        })
    });
    poll(&view, cx, 1000);
    assert_eq!(cx.update(|_, cx| view.read(cx).overview_needing()), 0);
}

#[gpui::test]
fn a_mark_rebuilds_within_the_second(cx: &mut gpui::TestAppContext) {
    let (view, cx) = settled(cx);
    cx.update(|_, cx| {
        Marks::update(cx, |marks| marks.toggle_pin(&key("planted")));
    });
    poll(&view, cx, 1000);
    assert!(rebuilt(&view, cx));
}

#[gpui::test]
fn a_mute_rebuilds_within_the_second(cx: &mut gpui::TestAppContext) {
    let (view, cx) = settled(cx);
    cx.update(|_, cx| {
        let (daemon, snapshot) = {
            let view = view.read(cx);
            let endpoint = &view.endpoints[view.selected_endpoint];
            (
                crate::notification_mutes::Daemon::of(endpoint),
                view.live.snapshot.clone().unwrap(),
            )
        };
        Mutes::update(cx, |mutes| {
            mutes.set(&daemon, &snapshot, Scope::Workspace, "w0", true, at(1000))
        });
    });
    poll(&view, cx, 1000);
    assert!(rebuilt(&view, cx));
}

#[gpui::test]
fn a_snooze_wakes_on_its_second(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    poll(&view, cx, 1000);
    let blocked = cx.update(|_, cx| {
        view.read(cx)
            .overview_rows(cx)
            .into_iter()
            .find(|row| row.state == State::Blocked)
            .unwrap()
            .key
    });
    cx.update(|_, cx| {
        let until = at(1001);
        Marks::update(cx, |marks| marks.snooze(&blocked, State::Blocked, until));
    });
    poll(&view, cx, 1000);
    assert_eq!(cx.update(|_, cx| view.read(cx).overview_needing()), 0);
    poll(&view, cx, 1001);
    assert_eq!(cx.update(|_, cx| view.read(cx).overview_needing()), 1);
}

#[gpui::test]
fn a_reopen_advances_on_a_tick_that_rebuilds_nothing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let (_, id) = close_working(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.live.status = ConnectionStatus::Disconnected;
            view.reopen_closed_agent(id, cx);
            view.poll_overview_at(at(1000), cx);
            view.overview.drawn.2 = PLANTED;
            view.overview.reopen.as_mut().unwrap().expire();
            view.poll_overview_at(at(1000), cx);
            assert!(!view.overview_reopening());
        })
    });
    assert!(!rebuilt(&view, cx));
}
