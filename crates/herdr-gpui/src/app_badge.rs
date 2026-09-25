//! A deduplicated Dock attention count across all windows, with a QA preview.

use crate::endpoint::Endpoint;
use gpui_kit::{App, Global, WindowId};
use herdr_client::ConnectTarget;
use herdr_client::protocol::{AgentStatus, ClientShellSnapshot};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
struct Contribution {
    snapshots: Vec<(Option<String>, Arc<ClientShellSnapshot>)>,
}

impl Contribution {
    fn update<'a>(
        &mut self,
        snapshots: impl Iterator<Item = (Option<&'a str>, &'a Arc<ClientShellSnapshot>)> + Clone,
    ) -> bool {
        // Surface-only updates keep the same Arc. Do not rescan agents every tick.
        if self
            .snapshots
            .iter()
            .map(|(host, snapshot)| (host.as_deref(), Arc::as_ptr(snapshot)))
            .eq(snapshots
                .clone()
                .map(|(host, snapshot)| (host, Arc::as_ptr(snapshot))))
        {
            return false;
        }
        self.snapshots = snapshots
            .map(|(host, snapshot)| (host.map(str::to_owned), snapshot.clone()))
            .collect();
        true
    }
}

#[derive(Default)]
struct Badge {
    windows: HashMap<WindowId, Contribution>,
    published: Option<usize>,
    preview: bool,
}

impl Global for Badge {}

impl Badge {
    fn publish(&mut self) {
        // Snapshot revisions are client-local. Agent state sequences are shared;
        // for the same sequence, an acknowledgement wins over a stale Done dot.
        let mut agents = HashMap::new();
        for (host, snapshot) in self.windows.values().flat_map(|window| &window.snapshots) {
            for agent in &snapshot.agents {
                let attention =
                    matches!(agent.agent_status, AgentStatus::Done | AgentStatus::Blocked);
                let current = agents
                    .entry((
                        host.as_deref(),
                        snapshot.boot_id.as_str(),
                        agent.pane_id.as_str(),
                    ))
                    .or_insert((agent.state_change_seq, attention));
                if agent.state_change_seq > current.0 {
                    *current = (agent.state_change_seq, attention);
                } else if agent.state_change_seq == current.0 {
                    current.1 &= attention;
                }
            }
        }
        let count = agents
            .values()
            .filter(|(_, attention)| *attention)
            .count()
            .max(if self.preview { 2 } else { 0 });
        if self.published != Some(count) {
            set_native(count);
            self.published = Some(count);
        }
    }
}

pub(super) fn install(cx: &mut App) {
    cx.default_global::<Badge>().publish();
    cx.on_action(|action: &crate::actions::SetBadgePreview, cx| {
        let badge = cx.default_global::<Badge>();
        badge.preview = action.enabled;
        badge.publish();
    });
    cx.on_window_closed(|cx, _| {
        let open = cx.windows();
        let badge = cx.default_global::<Badge>();
        badge
            .windows
            .retain(|id, _| open.iter().any(|window| window.window_id() == *id));
        badge.publish();
    })
    .detach();
}

pub(super) fn sync(window: WindowId, endpoints: &[Endpoint], cx: &mut App) {
    let badge = cx.default_global::<Badge>();
    let changed = badge.windows.entry(window).or_default().update(
        endpoints
            .iter()
            .filter(|endpoint| endpoint.enabled && endpoint.live.status.is_connected())
            .filter_map(|endpoint| {
                let host = match &endpoint.connection.target {
                    ConnectTarget::Ssh { target, .. } => Some(target.as_str()),
                    _ => None,
                };
                endpoint
                    .live
                    .snapshot
                    .as_ref()
                    .map(|snapshot| (host, snapshot))
            }),
    );
    if changed {
        badge.publish();
    }
}

#[cfg(not(test))]
fn set_native(count: usize) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
    NSApplication::sharedApplication(main_thread)
        .dockTile()
        .setBadgeLabel(label.as_deref());
}

// Headless tests must never initialize AppKit or change the test runner's Dock icon.
#[cfg(test)]
fn set_native(_: usize) {}

#[cfg(all(target_os = "macos", feature = "integration-test"))]
pub(super) fn verify_native() -> anyhow::Result<()> {
    use anyhow::Context as _;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    let main_thread = MainThreadMarker::new().context("badge check requires the main thread")?;
    let tile = NSApplication::sharedApplication(main_thread).dockTile();
    let previous = tile.badgeLabel();
    for count in [2, 1, 0] {
        set_native(count);
        let actual = tile.badgeLabel().map(|label| label.to_string());
        let valid = if count > 0 {
            actual.as_deref() == Some(count.to_string().as_str())
        } else {
            actual.as_deref().is_none_or(str::is_empty)
        };
        if !valid {
            tile.setBadgeLabel(previous.as_deref());
            anyhow::bail!("native Dock badge did not reflect count={count}");
        }
    }
    tile.setBadgeLabel(previous.as_deref());
    eprintln!("BADGE native PASS: Dock count changed from 2 to 1 and cleared");
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{
        sidebar::layout_tests::{fixture_window, snapshot},
        state::ConnectionStatus,
    };
    use herdr_client::ConnectTarget;

    #[gpui_kit::test]
    fn first_snapshot_counts_existing_attention_without_surface_or_notifications(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let handle = cx.add_window(fixture_window);
        handle
            .update(cx, |view, window, cx| {
                install(cx);
                let id = window.window_handle().window_id();
                sync(id, &view.endpoints, cx);
                assert_eq!(cx.global::<Badge>().published, Some(0));

                let mut initial = snapshot(2);
                initial.agents[0].agent_status = AgentStatus::Done;
                initial.agents[1].agent_status = AgentStatus::Blocked;
                view.endpoints[0]
                    .live
                    .apply(herdr_client::ClientEvent::Snapshot(Arc::new(
                        initial.clone(),
                    )));
                assert!(view.endpoints[0].live.surface.is_none());
                assert!(view.endpoints[0].live.notifications.is_empty());
                sync(id, &view.endpoints, cx);
                assert_eq!(cx.global::<Badge>().published, Some(2));

                initial.revision += 1;
                initial.agents[0].agent_status = AgentStatus::Idle;
                view.endpoints[0]
                    .live
                    .apply(herdr_client::ClientEvent::Snapshot(Arc::new(initial)));
                sync(id, &view.endpoints, cx);
                assert_eq!(cx.global::<Badge>().published, Some(1));
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn qa_preview_survives_polling_and_restores_daemon_attention(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use crate::actions::SetBadgePreview;

        let handle = cx.add_window(fixture_window);
        handle.update(cx, |_, _, cx| install(cx)).unwrap();
        // Application-level actions work even without a connected daemon.
        cx.update(|cx| cx.dispatch_action(&SetBadgePreview { enabled: true }));
        cx.run_until_parked();
        handle
            .update(cx, |view, window, cx| {
                sync(window.window_handle().window_id(), &view.endpoints, cx);
                assert_eq!(cx.global::<Badge>().published, Some(2));
            })
            .unwrap();
        cx.update(|cx| cx.dispatch_action(&SetBadgePreview { enabled: false }));
        cx.run_until_parked();
        cx.read_global::<Badge, _>(|badge, _| assert_eq!(badge.published, Some(0)));

        cx.update(|cx| cx.dispatch_action(&SetBadgePreview { enabled: true }));
        cx.run_until_parked();
        handle
            .update(cx, |view, window, cx| {
                let mut state = snapshot(2);
                state.agents[0].agent_status = AgentStatus::Done;
                view.endpoints[0].live.snapshot = Some(Arc::new(state));
                view.endpoints[0].live.status = ConnectionStatus::Connected;
                sync(window.window_handle().window_id(), &view.endpoints, cx);
            })
            .unwrap();
        cx.update(|cx| cx.dispatch_action(&SetBadgePreview { enabled: false }));
        cx.run_until_parked();
        cx.read_global::<Badge, _>(|badge, _| {
            assert!(!badge.preview);
            assert_eq!(badge.published, Some(1), "real attention remains visible");
        });
    }

    #[gpui_kit::test]
    fn only_done_and_blocked_need_attention_even_when_focused(cx: &mut gpui_kit::TestAppContext) {
        let handle = cx.add_window(fixture_window);
        let id = handle.window_id();
        let mut badge = Badge::default();
        for (status, expected) in [
            (AgentStatus::Done, 1),
            (AgentStatus::Idle, 0),
            (AgentStatus::Blocked, 1),
            (AgentStatus::Working, 0),
            (AgentStatus::Unknown, 0),
        ] {
            let mut state = snapshot(2);
            state.agents[0].agent_status = status;
            state.agents[0].focused = true;
            let state = Arc::new(state);
            let contribution = badge.windows.entry(id).or_default();
            assert!(contribution.update(std::iter::once((None, &state))));
            assert!(!contribution.update(std::iter::once((None, &state))));
            badge.publish();
            assert_eq!(badge.published, Some(expected));
        }
        let mut state = snapshot(2);
        state.agents[1].agent_status = AgentStatus::Done;
        let mut state = Arc::new(state);
        assert!(
            badge
                .windows
                .entry(id)
                .or_default()
                .update(std::iter::once((None, &state)))
        );
        badge.publish();
        assert_eq!(badge.published, Some(1));
        // A replacement snapshot need not change its revision for the cache to notice.
        Arc::make_mut(&mut state).agents.clear();
        assert!(
            badge
                .windows
                .entry(id)
                .or_default()
                .update(std::iter::once((None, &state)))
        );
        badge.publish();
        assert_eq!(badge.published, Some(0));
    }

    #[gpui_kit::test]
    fn count_drops_from_two_to_one_despite_a_lagging_duplicate_window(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let first = cx.add_window(fixture_window).window_id();
        let second = cx.add_window(fixture_window).window_id();
        let mut badge = Badge::default();
        let mut state = snapshot(2);
        state.revision = 500;
        for agent in &mut state.agents {
            agent.agent_status = AgentStatus::Done;
            agent.state_change_seq = 10;
        }
        let stale = Arc::new(state.clone());
        for id in [first, second] {
            badge
                .windows
                .entry(id)
                .or_default()
                .update(std::iter::once((None, &stale)));
        }
        badge.publish();
        assert_eq!(
            badge.published,
            Some(2),
            "two windows must not count four agents"
        );

        // A newly connected client can have a smaller projection revision.
        state.revision = 1;
        state.agents[0].agent_status = AgentStatus::Idle;
        let seen = Arc::new(state.clone());
        badge
            .windows
            .get_mut(&first)
            .unwrap()
            .update(std::iter::once((None, &seen)));
        badge.publish();
        assert_eq!(badge.published, Some(1));
        state.agents[1].agent_status = AgentStatus::Idle;
        let seen = Arc::new(state.clone());
        badge
            .windows
            .get_mut(&first)
            .unwrap()
            .update(std::iter::once((None, &seen)));
        badge.publish();
        assert_eq!(badge.published, Some(0));

        state.agents[0].agent_status = AgentStatus::Blocked;
        state.agents[0].state_change_seq = 11;
        let waiting = Arc::new(state);
        badge
            .windows
            .get_mut(&second)
            .unwrap()
            .update(std::iter::once((None, &waiting)));
        badge.publish();
        assert_eq!(
            badge.published,
            Some(1),
            "a new event supersedes an old acknowledgement"
        );

        // Separate hosts may report identical boot and pane IDs.
        badge
            .windows
            .get_mut(&first)
            .unwrap()
            .update(std::iter::once((Some("remote"), &waiting)));
        badge.publish();
        assert_eq!(badge.published, Some(2));
    }

    #[gpui_kit::test]
    fn hosts_disconnect_reconnect_and_disable_without_local_acknowledgement(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let handle = cx.add_window(fixture_window);
        handle
            .update(cx, |view, window, cx| {
                install(cx);
                let mut remote = Endpoint::new(
                    "remote".into(),
                    "Remote".into(),
                    ConnectTarget::Socket("/unused-badge.sock".into()),
                    true,
                );
                let mut state = snapshot(2);
                state.agents[0].agent_status = AgentStatus::Blocked;
                remote.live.snapshot = Some(Arc::new(state));
                remote.live.status = ConnectionStatus::Connected;
                remote.collapsed = true;
                view.endpoints.push(remote);
                view.sidebar_visible = false;
                view.active = true;
                view.toasts_hidden = true;
                let id = window.window_handle().window_id();
                sync(id, &view.endpoints, cx);
                assert_eq!(cx.global::<Badge>().published, Some(1));

                for status in [ConnectionStatus::Disconnected, ConnectionStatus::Detached] {
                    view.endpoints[1].live.status = status;
                    sync(id, &view.endpoints, cx);
                    assert_eq!(cx.global::<Badge>().published, Some(0));
                    view.endpoints[1].live.status = ConnectionStatus::Connected;
                    sync(id, &view.endpoints, cx);
                    assert_eq!(cx.global::<Badge>().published, Some(1));
                }
                view.endpoints[1].enabled = false;
                sync(id, &view.endpoints, cx);
                assert_eq!(cx.global::<Badge>().published, Some(0));
                view.endpoints[1].enabled = true;
                sync(id, &view.endpoints, cx);
                assert_eq!(cx.global::<Badge>().published, Some(1));
                view.endpoints.remove(1);
                sync(id, &view.endpoints, cx);
                assert_eq!(cx.global::<Badge>().published, Some(0));
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn closing_windows_removes_only_their_contribution(cx: &mut gpui_kit::TestAppContext) {
        let first = cx.add_window(fixture_window);
        let second = cx.add_window(fixture_window);
        let quiet = cx.add_window(fixture_window);
        first.update(cx, |_, _, cx| install(cx)).unwrap();
        for handle in [first, second] {
            handle
                .update(cx, |view, window, cx| {
                    let mut state = snapshot(2);
                    state.agents[0].agent_status = AgentStatus::Done;
                    view.endpoints[0].live.snapshot = Some(Arc::new(state));
                    view.endpoints[0].live.status = ConnectionStatus::Connected;
                    sync(window.window_handle().window_id(), &view.endpoints, cx);
                    assert_eq!(cx.global::<Badge>().published, Some(1));
                })
                .unwrap();
        }
        quiet
            .update(cx, |view, window, cx| {
                sync(window.window_handle().window_id(), &view.endpoints, cx);
                assert_eq!(cx.global::<Badge>().published, Some(1));
            })
            .unwrap();
        first
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();
        cx.read_global::<Badge, _>(|badge, _| {
            assert_eq!(badge.windows.len(), 2);
            assert_eq!(badge.published, Some(1));
        });
        second
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();
        cx.read_global::<Badge, _>(|badge, _| {
            assert_eq!(badge.windows.len(), 1);
            assert_eq!(badge.published, Some(0));
        });
        quiet
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();
        cx.read_global::<Badge, _>(|badge, _| {
            assert!(badge.windows.is_empty());
            assert_eq!(badge.published, Some(0));
        });
    }
}
