use super::*;
use crate::pane_menu::carry::{Availability, PaneState};
use crate::teleport::Error;

#[gpui::test]
fn non_repo_devices_are_listed_and_unavailable_rows_explain_why(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.endpoints[0].connection.target = ConnectTarget::Local;
        let mut outside = snapshot();
        outside.workspaces[0].worktree = None;
        view.live.snapshot = Some(Arc::new(outside.clone()));
        let mut disabled = device("ssh:disabled", "Disabled", ssh("disabled"), outside.clone());
        disabled.enabled = false;
        let mut offline = device("ssh:offline", "Offline", ssh("offline"), outside.clone());
        offline.live.status = ConnectionStatus::Disconnected;
        view.endpoints.extend([
            device("ssh:there", "There", ssh("there"), outside.clone()),
            device("ssh:repo", "Repo", ssh("repo"), snapshot()),
            device(
                "socket",
                "Socket",
                ConnectTarget::Socket("/tmp/unused.sock".into()),
                outside,
            ),
            disabled,
            offline,
        ]);
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_pane_menu("inactive", Point::default(), window, cx);
            let pane = v.menu.pane.as_mut().unwrap();
            assert_eq!(
                pane.target
                    .devices
                    .iter()
                    .map(|device| device.endpoint_id.as_str())
                    .collect::<Vec<_>>(),
                ["ssh:there", "ssh:repo"]
            );
            assert!(
                pane.target
                    .devices
                    .iter()
                    .all(|device| device.choice == Choice::SessionOnly)
            );
            pane.carry_availability = Some(Availability::ready(Ok(())));
            assert!(!pane.carry_availability.as_ref().unwrap().muted());
            v.activate_pane_menu(Action::MoveTo, window, cx);
            for refusal in [
                Error::SessionRequired,
                Error::OutsideHome {
                    path: "/outside".into(),
                },
            ] {
                let reason = refusal.to_string();
                let seen = PaneState::of(v.live.snapshot.as_deref().unwrap(), "inactive");
                let pane = v.menu.pane.as_mut().unwrap();
                pane.carry_availability = Some(Availability::ready_seen(Err(refusal), seen));
                assert!(pane.carry_availability.as_ref().unwrap().muted());
                assert!(!pane.target.devices.is_empty());
                pane.selected = Some(pane.rows(Some(Page::MovePane)) - 1);
                v.submit_move_picker(window, cx);
                assert_eq!(v.menu.page, Some(Page::MovePane));
                assert_eq!(
                    v.menu.pane.as_ref().unwrap().error.as_deref(),
                    Some(reason.as_str())
                );
                assert!(v.pane_carry.is_none());
            }
            let mut busy = v.live.snapshot.as_ref().unwrap().as_ref().clone();
            busy.agents[0].pane_id = "inactive".into();
            busy.agents[0].agent_status = AgentStatus::Working;
            v.live.snapshot = Some(Arc::new(busy));
            v.menu.pane.as_mut().unwrap().carry_availability = Some(Availability::ready(Ok(())));
            v.submit_move_picker(window, cx);
            assert!(
                v.menu
                    .pane
                    .as_ref()
                    .unwrap()
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("working")
            );
            assert!(v.pane_carry.is_none());
        });
    });
}

#[test]
fn only_a_refusal_made_under_another_pane_state_is_stale() {
    let mut snapshot = snapshot();
    snapshot.agents[0].pane_id = "inactive".into();
    let before = PaneState::of(&snapshot, "inactive");
    let refused = Availability::ready_seen(Err(Error::SessionRequired), before.clone());
    assert!(!refused.stale(&before));
    // The agent reports its session: the daemon bumps the state sequence.
    snapshot.agents[0].state_change_seq += 1;
    let after = PaneState::of(&snapshot, "inactive");
    assert!(refused.stale(&after));
    // An allowed move or a check still running is never re-asked.
    assert!(!Availability::ready_seen(Ok(()), before).stale(&after));
}

#[gpui::test]
fn choosing_a_stale_refusal_checks_again(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        // A target the script runner refuses before starting any process, so
        // the re-check can never reach a real daemon.
        view.endpoints[0].connection.target = ConnectTarget::Ssh {
            target: "-not-a-host".into(),
            session: "qa".into(),
        };
        let mut outside = snapshot();
        outside.workspaces[0].worktree = None;
        outside.agents[0].pane_id = "inactive".into();
        view.live.snapshot = Some(Arc::new(outside.clone()));
        view.endpoints
            .push(device("ssh:there", "There", ssh("there"), outside));
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_pane_menu("inactive", Point::default(), window, cx);
            let before = PaneState::of(v.live.snapshot.as_deref().unwrap(), "inactive");
            let pane = v.menu.pane.as_mut().unwrap();
            pane.carry_availability = Some(Availability::ready_seen(
                Err(Error::SessionRequired),
                before,
            ));
            v.activate_pane_menu(Action::MoveTo, window, cx);
            // The agent reports its session after the picker opened.
            let mut later = v.live.snapshot.as_ref().unwrap().as_ref().clone();
            later.agents[0].state_change_seq += 1;
            v.live.snapshot = Some(Arc::new(later));
            let pane = v.menu.pane.as_mut().unwrap();
            pane.selected = Some(pane.rows(Some(Page::MovePane)) - 1);
            v.submit_move_picker(window, cx);
            let pane = v.menu.pane.as_ref().unwrap();
            assert!(
                pane.error
                    .as_deref()
                    .is_some_and(|error| error.starts_with("Checking")),
                "{:?}",
                pane.error
            );
            assert!(v.pane_carry.is_none());
        });
    });
}
