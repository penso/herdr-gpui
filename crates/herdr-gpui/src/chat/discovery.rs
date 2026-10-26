//! Which panes run a chat program this window may attach to.

use super::{MAX_CHATS, Route, Target};
use crate::state::LiveState;
use herdr_chat_protocol::{EndpointId, TOKEN};
use herdr_client::{ConnectTarget, protocol::ClientShellSnapshot};
use std::collections::HashSet;

/// Whether the daemon behind `target` runs on this machine as this user, so
/// its panes' chat endpoints are this user's own: the trust boundary local Git
/// and checkpoints use. A socket given by path may be a forward from another
/// host, and every remote or cloud daemon's programs listen elsewhere.
///
/// Windows cannot verify the daemon's peer, so there the check falls to the
/// chat pipe itself, which is refused unless this user owns it.
pub(crate) fn local(target: &ConnectTarget, live: &LiveState) -> bool {
    match target {
        ConnectTarget::Local | ConnectTarget::Session { .. } => {
            (live.local_daemon_peer || cfg!(windows)) && live.status.is_connected()
        }
        _ => false,
    }
}

/// How this window reaches the chat programs of the daemon behind `target`:
/// on this machine as for [`local`], or over SSH on the host an SSH
/// connection runs the daemon on. A socket given by path, a WSL
/// distribution, and a cloud machine have no route yet.
pub(crate) fn route(target: &ConnectTarget, live: &LiveState) -> Option<Route> {
    match target {
        _ if local(target, live) => Some(Route::Local),
        ConnectTarget::Ssh { target, .. } if live.status.is_connected() => {
            Some(Route::Ssh(target.clone()))
        }
        _ => None,
    }
}

/// The program a pane's metadata names, when its token holds a valid id.
pub(crate) fn endpoint(tokens: &[(String, String)]) -> Option<EndpointId> {
    tokens
        .iter()
        .find(|(name, _)| name == TOKEN)
        .and_then(|(_, value)| value.parse().ok())
}

/// The chat panes of a reachable daemon's snapshot, in its order, one per
/// pane and at most [`MAX_CHATS`], each reached by `route`. A daemon with
/// no route has none.
pub(crate) fn targets(snapshot: &ClientShellSnapshot, route: Option<&Route>) -> Vec<Target> {
    let Some(route) = route else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    snapshot
        .agents
        .iter()
        .filter_map(|agent| {
            Some(Target {
                endpoint: endpoint(&agent.tokens)?,
                existing: None,
                pane_id: agent.pane_id.clone(),
                route: route.clone(),
            })
        })
        .filter(|target| seen.insert(target.pane_id.clone()))
        .take(MAX_CHATS)
        .collect()
}

/// Verified CLI bindings add rich views to existing terminals. Real ACP
/// endpoints win, and a vanished/changed agent cannot retain its old overlay.
pub(crate) fn with_existing(
    mut targets: Vec<Target>,
    snapshot: &ClientShellSnapshot,
    route: Option<&Route>,
    bindings: &[super::existing::Binding],
) -> Vec<Target> {
    let Some(route) = route else { return targets };
    extend(&mut targets, bindings, |binding| {
        if !snapshot.agents.iter().any(|agent| {
            agent.pane_id == binding.pane_id && agent.agent.as_deref() == Some(binding.agent.name())
        }) {
            return None;
        }
        Some(Target {
            endpoint: uuid::Uuid::parse_str(&binding.session_id).ok()?.into(),
            existing: Some(binding.clone()),
            pane_id: binding.pane_id.clone(),
            route: route.clone(),
        })
    });
    targets
}

/// Trusted, explicit sidecars add a view to an existing agent without writing
/// its lifecycle metadata. ACP's own endpoint always takes precedence.
pub(crate) fn with_mirrors(
    mut targets: Vec<Target>,
    snapshot: &ClientShellSnapshot,
    route: Option<&Route>,
    mirrors: &[crate::config::chat::TranscriptMirror],
    render_existing: bool,
) -> Vec<Target> {
    // A static mirror cannot stand in for a failed/pending live lookup: the
    // pane may already contain a different conversation. Keep its terminal.
    if render_existing {
        return targets;
    }
    let Some(route) = route else { return targets };
    extend(&mut targets, mirrors, |mirror| {
        let matches_host = match (route, mirror.host.as_deref()) {
            (Route::Local, None) => true,
            (Route::Ssh(host), Some(expected)) => host == expected,
            _ => false,
        };
        if !matches_host
            || !snapshot
                .agents
                .iter()
                .any(|agent| agent.pane_id == mirror.pane)
        {
            return None;
        }
        Some(Target {
            endpoint: mirror.endpoint.parse().ok()?,
            existing: None,
            pane_id: mirror.pane.clone(),
            route: route.clone(),
        })
    });
    targets
}

/// Adds the target `target` makes of each of the first [`MAX_CHATS`]
/// candidates, until there are [`MAX_CHATS`] targets. A pane keeps the
/// target it already has, so earlier sources win.
fn extend<T>(
    targets: &mut Vec<Target>,
    candidates: &[T],
    mut target: impl FnMut(&T) -> Option<Target>,
) {
    for candidate in candidates.iter().take(MAX_CHATS) {
        if targets.len() >= MAX_CHATS {
            break;
        }
        if let Some(target) = target(candidate)
            && !targets.iter().any(|known| known.pane_id == target.pane_id)
        {
            targets.push(target);
        }
    }
}
