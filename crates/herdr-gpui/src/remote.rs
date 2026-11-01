//! Remote control, Herdr UI's side: the `herdr-remote-control` daemon serves this
//! machine's chats to paired phones, and Herdr UI installs it as a user
//! service, watches it, pairs devices, and revokes them. Only the daemon's
//! control protocol is used here (`herdr_remote_control::control`), from background
//! threads; Tokio never reaches the app.
//!
//! The service, the `[remote]` table, and the pairing state and QR code live
//! in `herdr_remote_control::app`, with no GPUI. Here, [`supervisor`] keeps
//! the service in line with `[remote]` and polls its status, and
//! [`dialog`], [`revoke`], and [`indicator`] put them in the window.

mod dialog;
mod indicator;
pub(crate) use indicator::MENU_WIDTH;
pub(crate) mod links;
pub(crate) mod nearby;
mod revoke;
pub(crate) mod supervisor;

pub(crate) use dialog::{PairingDialog, addresses};
pub use herdr_remote_control::app::Error;
pub(crate) use herdr_remote_control::app::{Result, pairing, qr, service};
use herdr_remote_control::control::Device;
pub(crate) use revoke::Revoke;
pub(crate) use supervisor::{Health, Supervisor};

use crate::i18n::fl;
use std::path::PathBuf;

/// The daemon's private state directory: the app's own, under `remote/`.
pub(crate) fn state_dir() -> Result<PathBuf> {
    crate::preferences::state_dir()
        .map(|dir| dir.join("remote"))
        .ok_or(Error::NoStateDir)
}

/// Where this user's service goes and what it runs. Reads the environment
/// and looks for the daemon on disk: run it off the UI thread.
fn host(state_dir: PathBuf) -> crate::Result<service::Host> {
    let manager = service::Manager::current()?;
    let home = crate::config::home()?;
    let config_home = herdr_client::local_var("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    Ok(service::Host {
        file: manager.file(&home, &config_home, &state_dir),
        manager,
        state_dir,
        program: crate::bundled::locate(service::PROGRAM),
        release: crate::RELEASE_BUILD.then(|| crate::APP_VERSION.to_owned()),
    })
}

/// "Last seen 2 h ago · notifications on".
pub(crate) fn device_line(device: &Device, now_ms: u64) -> String {
    let (seen, ago) = match device.last_seen_ms {
        _ if device.connected => ("connected", String::new()),
        Some(seen) => ("seen", crate::ago::text(now_ms.saturating_sub(seen) / 1000)),
        None => ("never", String::new()),
    };
    let push = if device.push { "on" } else { "off" };
    fl!("remote-device-line", seen = seen, ago = ago, push = push)
}

/// The other machines a phone is paired with from here, or being paired
/// with: "Also on devbox, gpu box · pairing lab · failed on ci". `None`
/// when there are none.
pub(crate) fn hosts_line(device: &Device) -> Option<String> {
    use herdr_remote_control::control::hosts::HostState;
    let names = |wanted: fn(&HostState) -> bool| {
        device
            .hosts
            .iter()
            .filter(|link| wanted(&link.state))
            .map(|link| crate::notifications::safe_text(&link.name, 64))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let parts: Vec<String> = [
        (
            "Also on ",
            names(|state| matches!(state, HostState::Paired(_))),
        ),
        (
            "pairing ",
            names(|state| matches!(state, HostState::Pending | HostState::Asking)),
        ),
        (
            "failed on ",
            names(|state| matches!(state, HostState::Failed { .. })),
        ),
    ]
    .into_iter()
    .filter(|(_, names)| !names.is_empty())
    .map(|(label, names)| format!("{label}{names}"))
    .collect();
    (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
}

/// Milliseconds since the Unix epoch, now.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests;
