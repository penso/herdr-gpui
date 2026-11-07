//! Herdr sessions inside the WSL distributions saved as devices on this
//! Windows computer, so a phone sees them beside the native ones. They run
//! on this computer, and sleep with it, but behind `wsl.exe`: the layout and
//! terminals come over the client bridge and requests over the API bridge.
//!
//! Only sessions already running are followed, in distributions already
//! running, so nothing here boots a distribution or starts a server. A
//! distribution's chat programs cannot be reached from here, so its chats
//! are listed as terminals (`layout::Chats`).

use sheprd_client::{ConnectTarget, RemoteSession, WslHost};
use std::path::Path;

/// Between a session and its distribution in a followed name. Neither name
/// can hold it, so no session on this computer can be mistaken for one.
const SEPARATOR: char = '@';
/// The most sessions followed across distributions: each holds a `wsl.exe`.
/// A scan lists no more, and [`super::Sessions::update`] holds no more, as
/// sessions it already follows can fall out of a scan and stay.
pub(in crate::hub) const MAX_SESSIONS: usize = 32;

/// `SESSION@DISTRO`, as the session's ids carry it.
fn name(distro: &str, session: &str) -> String {
    format!("{session}{SEPARATOR}{distro}")
}

/// Whether `name` is a session inside a distribution.
pub(in crate::hub) fn is_wsl(name: &str) -> bool {
    name.contains(SEPARATOR)
}

/// The sessions to follow now, of the distributions saved in `catalog` (found
/// from this process's environment when `None`). Blocking; asks nothing of a
/// computer with no distribution saved.
pub(super) fn listed(catalog: Option<&Path>) -> Vec<(String, ConnectTarget)> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let saved = match catalog.map_or_else(
        || sheprd_client::load_wsl_hosts(false),
        sheprd_client::load_wsl_hosts_at,
    ) {
        Ok(saved) => saved.hosts,
        Err(error) => {
            tracing::warn!(%error, "could not read the saved WSL distributions");
            return Vec::new();
        }
    };
    if saved.is_empty() {
        return Vec::new();
    }
    let running = match sheprd_client::list_running_distros() {
        Ok(running) => running,
        Err(error) => {
            tracing::warn!(%error, "could not list the running WSL distributions");
            return Vec::new();
        }
    };
    wanted(
        &saved,
        &running,
        |distro| match sheprd_client::list_distro_sessions(distro) {
            Ok(sessions) => sessions,
            Err(error) => {
                tracing::warn!(%distro, %error, "could not list a WSL distribution's sessions");
                Vec::new()
            }
        },
    )
}

/// The running sessions of the distributions both `saved` and `running`, as
/// `sessions` lists them.
pub(in crate::hub) fn wanted(
    saved: &[WslHost],
    running: &[String],
    mut sessions: impl FnMut(&str) -> Vec<RemoteSession>,
) -> Vec<(String, ConnectTarget)> {
    saved
        .iter()
        .filter(|host| running.contains(&host.distro))
        .flat_map(|host| {
            sessions(&host.distro)
                .into_iter()
                .filter(|session| session.running)
                .map(|session| {
                    (
                        name(&host.distro, &session.name),
                        ConnectTarget::Wsl {
                            distro: host.distro.clone(),
                            session: session.name,
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .take(MAX_SESSIONS)
        .collect()
}
