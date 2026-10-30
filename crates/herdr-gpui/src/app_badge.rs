//! A deduplicated attention count across all windows, with a QA preview: the
//! Dock badge on macOS and the taskbar overlay on Windows. Agents in a
//! workspace or tab the user muted are not counted.

use crate::{
    endpoint::Endpoint,
    notification_mutes::{Daemon, Mutes, Silenced},
};
use gpui::{App, BorrowAppContext, Global, WindowId};
use herdr_client::ConnectTarget;
use herdr_client::protocol::{AgentStatus, ClientShellSnapshot};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// One snapshot a window counts, and its agents the user muted.
struct Shown {
    host: Option<String>,
    snapshot: Arc<ClientShellSnapshot>,
    /// Panes whose workspace or tab is muted: they never need attention here.
    silent: HashSet<String>,
}

#[derive(Default)]
struct Contribution {
    snapshots: Vec<Shown>,
    /// The mutes' revision the silent panes were found under.
    mutes: u64,
}

impl Contribution {
    /// Takes the window's `snapshots`, each with its host and the endpoint
    /// it came from, under the app's `mutes`, empty or not: lifting the
    /// last mute is a change too. Returns whether the count may have
    /// changed.
    fn update<'a>(
        &mut self,
        snapshots: impl Iterator<Item = (Option<&'a str>, &'a Arc<ClientShellSnapshot>, &'a Endpoint)>
        + Clone,
        mutes: Option<&Mutes>,
    ) -> bool {
        let revision = mutes.map_or(0, Mutes::revision);
        // Surface-only updates keep the same Arc. Do not rescan agents every tick.
        if self.mutes == revision
            && self
                .snapshots
                .iter()
                .map(|shown| (shown.host.as_deref(), Arc::as_ptr(&shown.snapshot)))
                .eq(snapshots
                    .clone()
                    .map(|(host, snapshot, _)| (host, Arc::as_ptr(snapshot))))
        {
            return false;
        }
        self.mutes = revision;
        self.snapshots = snapshots
            .map(|(host, snapshot, endpoint)| Shown {
                host: host.map(str::to_owned),
                snapshot: Arc::clone(snapshot),
                silent: mutes
                    .filter(|mutes| !mutes.is_empty())
                    .map(|mutes| silent(&mutes.silenced(&Daemon::of(endpoint), snapshot), snapshot))
                    .unwrap_or_default(),
            })
            .collect();
        true
    }
}

/// The panes of `snapshot`'s agents that `silenced` keeps quiet.
fn silent(silenced: &Silenced<'_>, snapshot: &ClientShellSnapshot) -> HashSet<String> {
    if silenced.is_empty() {
        return HashSet::new();
    }
    snapshot
        .agents
        .iter()
        .filter(|agent| {
            silenced.notice(
                Some(&agent.workspace_id),
                Some(&agent.tab_id),
                Some(&agent.pane_id),
            )
        })
        .map(|agent| agent.pane_id.clone())
        .collect()
}

#[derive(Default)]
struct Badge {
    windows: HashMap<WindowId, Contribution>,
    published: Option<usize>,
    preview: bool,
    permission_requested: bool,
}

impl Global for Badge {}

impl Badge {
    fn publish(&mut self) {
        // Snapshot revisions are client-local. Agent state sequences are shared;
        // for the same sequence, an acknowledgement wins over a stale Done dot.
        let mut agents = HashMap::new();
        for Shown {
            host,
            snapshot,
            silent,
        } in self.windows.values().flat_map(|window| &window.snapshots)
        {
            for agent in &snapshot.agents {
                let attention =
                    matches!(agent.agent_status, AgentStatus::Done | AgentStatus::Blocked)
                        && !silent.contains(&agent.pane_id);
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
            // Asked on the first badge rather than at launch, so an app that
            // never needs attention never prompts.
            if count > 0 && !self.permission_requested {
                self.permission_requested = true;
                request_permission();
            }
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
        #[cfg(windows)]
        crate::taskbar::button::retain(&open);
        let badge = cx.default_global::<Badge>();
        badge
            .windows
            .retain(|id, _| open.iter().any(|window| window.window_id() == *id));
        badge.publish();
    })
    .detach();
}

pub(super) fn sync(window: WindowId, endpoints: &[Endpoint], cx: &mut App) {
    let shown = endpoints
        .iter()
        .filter(|endpoint| endpoint.enabled && endpoint.live.status.is_connected())
        .filter_map(|endpoint| {
            let host = match &endpoint.connection.target {
                ConnectTarget::Ssh { target, .. } => Some(target.as_str()),
                ConnectTarget::Wsl { distro, .. } => Some(distro.as_str()),
                _ => None,
            };
            endpoint
                .live
                .snapshot
                .as_ref()
                .map(|snapshot| (host, snapshot, endpoint))
        });
    cx.default_global::<Badge>();
    // Leased out of the app, so the mutes can be read beside it.
    cx.update_global::<Badge, _>(|badge, cx| {
        let changed = badge
            .windows
            .entry(window)
            .or_default()
            .update(shown, Mutes::of(cx));
        if changed {
            badge.publish();
        }
    });
}

#[cfg(all(not(test), target_os = "macos"))]
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

#[cfg(all(not(test), windows))]
fn set_native(count: usize) {
    crate::taskbar::button::set_overlay(count);
}

// Headless tests must never initialize AppKit or change the test runner's Dock icon.
#[cfg(test)]
fn set_native(_: usize) {}

/// Once an app is registered with the notification center, macOS hides its
/// Dock badge unless the badge option was authorized. GPUI requests only alert
/// and sound when it posts a system notification, so the badge label would be
/// set but never drawn. This request asks for the union: whichever request
/// macOS answers first decides the permission, and a later request with other
/// options is not prompted for again.
#[cfg(all(not(test), target_os = "macos"))]
fn request_permission() {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSBundle, NSError};
    use objc2_user_notifications::{UNAuthorizationOptions, UNUserNotificationCenter};

    // The notification center aborts the process outside an app bundle,
    // which is how `cargo run` builds start.
    if NSBundle::mainBundle().bundleIdentifier().is_none() {
        return;
    }
    // Runs on a notification-center queue, not the UI thread.
    let completion = RcBlock::new(|granted: Bool, _: *mut NSError| {
        if !granted.as_bool() {
            tracing::info!("Dock badge permission was not granted");
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert
                | UNAuthorizationOptions::Sound
                | UNAuthorizationOptions::Badge,
            &completion,
        );
}

/// Windows asks no permission for a taskbar overlay.
#[cfg(all(not(test), windows))]
fn request_permission() {}

#[cfg(test)]
thread_local! {
    static PERMISSION_REQUESTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// Headless tests must never register the test runner with the notification center.
#[cfg(test)]
fn request_permission() {
    PERMISSION_REQUESTS.set(PERMISSION_REQUESTS.get() + 1);
}

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
mod tests;
