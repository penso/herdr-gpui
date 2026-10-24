// Not a glob of the parent: GPUI's glob would shadow the `#[test]` that
// `#[gpui::test]` expands to.
use crate::{
    fan_out::{
        Journal,
        job::{Checkout, Progress, Report},
        journal::Record,
        plan::{self, Picks},
        state::Origin,
    },
    menu::Page,
    teleport::host_for,
};

/// A journal left by a previous launch whose first lane was cut short
/// after its worktree was created, on `endpoint_id`.
fn left_behind(path: &std::path::Path, endpoint_id: &str) -> Journal {
    let host = host_for(&herdr_client::ConnectTarget::Local).unwrap();
    let origin = Origin {
        endpoint_id: endpoint_id.to_owned(),
        endpoint_label: "This Mac".into(),
        host: host.clone(),
        workspace_id: "w3".into(),
        repo_key: "/tmp/repo/.git".into(),
        repo_label: "repo".into(),
        base: "HEAD".into(),
    };
    let mut picks = Picks::default();
    picks.add(crate::teleport::AgentKind::Claude);
    let mut record = Record::new(1, &origin, "fix the bug", &plan::lanes(&picks, 7));
    record.advance(&Report::Lane(
        0,
        Progress::Created(Checkout {
            endpoint_id: endpoint_id.to_owned(),
            host,
            workspace_id: "w4".into(),
            path: "/tmp/lane".into(),
            pane_id: "p4".into(),
        }),
    ));
    Journal::at(Some(path.to_owned()))
        .begin(record)
        .unwrap()
        .unwrap()
        .write()
        .unwrap();
    Journal::at(Some(path.to_owned()))
}

#[gpui::test]
fn an_interrupted_fan_out_comes_back_and_waits_for_the_user(cx: &mut gpui::TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fan-outs.json");
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let journal = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            let journal = left_behind(&path, &view.endpoints[0].id);
            journal.clone().install(cx);
            view.poll_fan_out(window, cx);
            let fan_out = view.fan_out.as_ref().unwrap();
            assert!(!fan_out.composing());
            assert_eq!(fan_out.interrupted(), 1);
            let (flash, _) = view.flash.as_ref().unwrap();
            assert!(flash.text.contains("interrupted"), "{}", flash.text);
            // Shown as the workspace menu shows a comparison.
            view.menu.page = Some(Page::FanOut);
            window.focus(&view.menu.focus, cx);
            journal
        })
    });
    cx.run_until_parked();
    // The lane is offered to be finished, and nothing ran by itself.
    assert!(cx.debug_bounds("fan-out-resume-0").is_some());
    assert!(cx.debug_bounds("fan-out-keep-0").is_some());
    let forget = cx.debug_bounds("fan-out-forget").unwrap();
    cx.simulate_click(forget.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(view.read(cx).fan_out.is_none());
        assert_eq!(view.read(cx).menu.page, None);
    });
    assert!(journal.records().is_empty());
}

#[gpui::test]
fn a_restored_lane_is_scripted_only_once_its_daemon_shows_its_workspace(
    cx: &mut gpui::TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fan-outs.json");
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            view.live.status = crate::state::ConnectionStatus::Connected;
            left_behind(&path, &view.endpoints[0].id).install(cx);
            view.poll_fan_out(window, cx);
            view.menu.page = Some(Page::FanOut);
            window.focus(&view.menu.focus, cx);
        })
    });
    cx.run_until_parked();
    // The fixture's daemon has a "w4", but on another branch: a reused ID.
    let resume = cx.debug_bounds("fan-out-resume-0").unwrap();
    cx.simulate_click(resume.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        let fan_out = view.fan_out.as_ref().unwrap();
        assert_eq!(fan_out.error.as_deref(), Some(super::LANE_NOT_FOUND));
        assert_eq!(fan_out.interrupted(), 1, "nothing was sent");
    });
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let branch = view.fan_out.as_ref().unwrap().lanes[0].lane.branch.clone();
            let mut snapshot = crate::sidebar::layout_tests::snapshot(5);
            snapshot.workspaces[4].branch = Some(branch);
            view.live.snapshot = Some(std::sync::Arc::new(snapshot));
            assert!(view.fan_out_lanes_found([0]));
            view.live.status = crate::state::ConnectionStatus::Disconnected;
            assert!(
                !view.fan_out_lanes_found([0]),
                "an unconnected host shows nothing"
            );
        })
    });
}
