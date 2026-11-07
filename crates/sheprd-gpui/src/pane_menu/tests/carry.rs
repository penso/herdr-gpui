use super::*;

#[path = "carry_session.rs"]
mod carry_session;
use crate::teleport::carry::Choice;
use crate::{endpoint::Endpoint, state::ConnectionStatus};
use sheprd_client::{ConnectTarget, protocol::AgentStatus};

fn device(id: &str, label: &str, target: ConnectTarget, snapshot: ClientShellSnapshot) -> Endpoint {
    let mut endpoint = Endpoint::new(id.into(), label.into(), target, true);
    endpoint.live.snapshot = Some(Arc::new(snapshot));
    endpoint.live.status = ConnectionStatus::Connected;
    endpoint
}

fn ssh(target: &str) -> ConnectTarget {
    ConnectTarget::Ssh {
        target: target.into(),
        session: "default".into(),
    }
}

fn picker_rows(view: &SheprdWindow) -> usize {
    view.menu.pane.as_ref().unwrap().rows(Some(Page::MovePane))
}

/// The picker lists, after this device's places, every other device that
/// can be scripted and has the pane's repository open; a pane outside any
/// repository lists those devices for a session-only move.
#[gpui::test]
fn devices_with_the_repository_follow_this_devices_places(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.endpoints[0].connection.target = ConnectTarget::Local;
        view.live.snapshot = Some(Arc::new(snapshot()));
        let mut elsewhere = snapshot();
        elsewhere.workspaces[0].worktree = None;
        view.endpoints.extend([
            device("ssh:there", "There", ssh("there"), snapshot()),
            // Neither the repository nor a host to script.
            device("ssh:elsewhere", "Elsewhere", ssh("elsewhere"), elsewhere),
            device(
                "socket",
                "Socket",
                ConnectTarget::Socket("/tmp/herdr.sock".into()),
                snapshot(),
            ),
        ]);
        let mut offline = device("ssh:offline", "Offline", ssh("offline"), snapshot());
        offline.live.status = ConnectionStatus::Disconnected;
        view.endpoints.push(offline);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_pane_menu("inactive", Point::default(), window, cx);
            let pane = v.menu.pane.as_ref().unwrap();
            let device = |choice, place: &str| crate::pane_menu::carry::Device {
                endpoint_id: "ssh:there".into(),
                label: "There".into(),
                choice,
                place: place.into(),
            };
            // A new worktree, then the device's space of the repository.
            assert_eq!(
                pane.target.devices,
                [
                    device(Choice::NewWorktree, "New Worktree"),
                    device(
                        Choice::Space {
                            workspace_id: "w1".into()
                        },
                        "repo"
                    ),
                ]
            );
            let here = pane.target.destinations().len();
            v.activate_pane_menu(Action::MoveTo, window, cx);
            assert_eq!(picker_rows(v), here + 2);
        });
        window.draw(cx).clear(cx);
    });
    let last = cx.update(|_, cx| picker_rows(view.read(cx)) - 1);
    let selector: &'static str = format!("pane-move-place-{last}").leak();
    assert!(cx.debug_bounds(selector).is_some());
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.dismiss_menu(window, cx);
            let mut outside = snapshot();
            outside.workspaces[0].worktree = None;
            v.live.snapshot = Some(Arc::new(outside));
            v.open_pane_menu("inactive", Point::default(), window, cx);
            let devices = &v.menu.pane.as_ref().unwrap().target.devices;
            assert!(
                devices
                    .iter()
                    .all(|device| device.choice == Choice::SessionOnly)
            );
            assert_eq!(
                devices
                    .iter()
                    .map(|device| device.endpoint_id.as_str())
                    .collect::<Vec<_>>(),
                ["ssh:there", "ssh:elsewhere"]
            );
        });
    });
}

/// A working agent's session is still being written, so choosing another
/// device says to wait rather than starting a review.
#[gpui::test]
fn a_working_agent_waits_to_change_devices(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.endpoints[0].connection.target = ConnectTarget::Local;
        let mut working = snapshot();
        working.agents[0].pane_id = "inactive".into();
        working.agents[0].agent_status = AgentStatus::Working;
        view.live.snapshot = Some(Arc::new(working));
        view.endpoints
            .push(device("ssh:there", "There", ssh("there"), snapshot()));
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_pane_menu("inactive", Point::default(), window, cx);
            v.activate_pane_menu(Action::MoveTo, window, cx);
            let last = picker_rows(v) - 1;
            v.menu.pane.as_mut().unwrap().selected = Some(last);
            v.submit_move_picker(window, cx);
            assert_eq!(v.menu.page, Some(Page::MovePane));
            assert!(v.pane_carry.is_none());
            let error = v.menu.pane.as_ref().unwrap().error.clone().unwrap();
            assert!(error.contains("working"), "{error}");
        });
    });
}

/// After a move, the window follows to the moved pane, which in an existing
/// space is a new tab rather than the space's active one: focusing the
/// space alone does not finish the follow.
#[gpui::test]
fn the_window_follows_to_the_moved_pane_not_only_its_space(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot()));
        view
    });
    let follow = |view: &mut SheprdWindow| {
        view.teleport_follow = Some(crate::teleport::Follow::pane(
            crate::endpoint::LOCAL.into(),
            "w1".into(),
            "inactive".into(),
        ));
    };
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            // The space is focused, but another of its panes is.
            assert_eq!(
                v.live
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .focused_workspace_id
                    .as_deref(),
                Some("w1")
            );
            follow(v);
            v.poll_teleport(window, cx);
            assert!(v.teleport_follow.is_some());

            let mut landed = snapshot();
            landed.focused_pane_id = Some("inactive".into());
            v.live.snapshot = Some(Arc::new(landed));
            follow(v);
            v.poll_teleport(window, cx);
            assert!(v.teleport_follow.is_none());
        });
    });
}
