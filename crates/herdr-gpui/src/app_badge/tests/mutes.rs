use super::*;
use crate::notification_mutes::Scope;

#[gpui::test]
fn muted_agents_leave_the_badge_and_unmuting_counts_them_again(cx: &mut gpui::TestAppContext) {
    let handle = cx.add_window(fixture_window);
    handle
        .update(cx, |view, window, cx| {
            install(cx);
            let id = window.window_handle().window_id();
            // Agent p0 is in workspace w0, tab t0; p1 in workspace w1.
            let mut state = snapshot(2);
            state.agents[0].agent_status = AgentStatus::Done;
            state.agents[1].agent_status = AgentStatus::Blocked;
            let state = Arc::new(state);
            view.endpoints[0].live.snapshot = Some(state.clone());
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(2));

            let daemon = Daemon::of(&view.endpoints[0]);
            let set = |scope, target: &str, muted, cx: &mut App| {
                assert!(Mutes::update(cx, |mutes| {
                    mutes.set(
                        &daemon,
                        &state,
                        scope,
                        target,
                        muted,
                        std::time::SystemTime::now(),
                    )
                }));
            };
            // The same snapshot: only the mutes changed.
            set(Scope::Tab, "t0", true, cx);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));
            set(Scope::Workspace, "w1", true, cx);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(0));
            set(Scope::Tab, "t0", false, cx);
            set(Scope::Workspace, "w1", false, cx);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(2));
            // A muted workspace covers every agent in it.
            set(Scope::Workspace, "w0", true, cx);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));
        })
        .unwrap();
}

#[gpui::test]
fn a_mute_on_one_host_does_not_quiet_the_same_ids_on_another(cx: &mut gpui::TestAppContext) {
    let handle = cx.add_window(fixture_window);
    handle
        .update(cx, |view, window, cx| {
            install(cx);
            let id = window.window_handle().window_id();
            let mut state = snapshot(2);
            state.agents[0].agent_status = AgentStatus::Done;
            let state = Arc::new(state);
            view.endpoints[0].live.snapshot = Some(state.clone());
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            let mut remote = Endpoint::new(
                "remote".into(),
                "Remote".into(),
                ConnectTarget::Ssh {
                    target: "box".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = Some(state.clone());
            remote.live.status = ConnectionStatus::Connected;
            view.endpoints.push(remote);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(2), "one per host");
            let daemon = Daemon::of(&view.endpoints[1]);
            Mutes::update(cx, |mutes| {
                mutes.set(
                    &daemon,
                    &state,
                    Scope::Workspace,
                    "w0",
                    true,
                    std::time::SystemTime::now(),
                )
            });
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));
        })
        .unwrap();
}

/// Mutes read back at launch start at revision 0, as an empty set does, so
/// lifting the last of them must still count its agents again.
#[gpui::test]
fn unmuting_the_last_mute_read_at_launch_counts_its_agents_again(cx: &mut gpui::TestAppContext) {
    let handle = cx.add_window(fixture_window);
    handle
        .update(cx, |view, window, cx| {
            install(cx);
            let id = window.window_handle().window_id();
            let mut state = snapshot(2);
            state.agents[0].agent_status = AgentStatus::Done;
            state.agents[1].agent_status = AgentStatus::Blocked;
            let state = Arc::new(state);
            view.endpoints[0].live.snapshot = Some(state.clone());
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            let daemon = Daemon::of(&view.endpoints[0]);
            let now = std::time::SystemTime::now();
            Mutes::update(cx, |mutes| {
                mutes.set(&daemon, &state, Scope::Workspace, "w1", true, now)
            });
            let loaded = Mutes::of(cx).unwrap().reloaded();
            assert_eq!(loaded.revision(), 0);
            cx.set_global(loaded);
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(1));
            assert!(Mutes::update(cx, |mutes| {
                mutes.set(&daemon, &state, Scope::Workspace, "w1", false, now)
            }));
            assert!(Mutes::any(cx).is_none(), "nothing is muted any more");
            sync(id, &view.endpoints, cx);
            assert_eq!(cx.global::<Badge>().published, Some(2));
        })
        .unwrap();
}
