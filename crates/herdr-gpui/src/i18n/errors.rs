//! Errors as the person using the app reads them (docs/i18n.md). Every error
//! enum that reaches a window keeps its `thiserror` English `Display` for
//! logs, and [`Localize`] picks its message by variant where it is shown.
//! An enum wrapping another crate's error delegates to that error's own
//! mapping, so each failure has one message, as it has one definition.
//!
//! The mappings live here, one file and one `error-<name>.ftl` per crate or
//! feature, rather than beside each enum: crates below the app have no
//! messages, and the app's feature modules are migrated on their own.
// i18n: migrated

mod automations;
mod backdrop;
mod caffeine;
mod chat;
mod client;
mod clipboard;
#[cfg(feature = "cloud")]
mod cloud_batch;
mod cloud_setup;
mod code_server;
mod config;
mod control;
mod credentials;
mod daemon;
mod devices;
mod dispatch;
mod editor;
mod file_drop;
mod github;
mod host_build;
mod keymap;
#[cfg(windows)]
mod launch_at_login;
mod listening_ports;
mod mods;
mod nearby;
mod notification_mutes;
mod orchestrator;
mod overview;
mod palette;
mod pane_carry;
mod pane_move;
mod port_forward;
mod processes;
mod protocol;
mod px0;
mod remote;
mod remote_control;
mod review;
mod root;
#[cfg(unix)]
mod setup_relay;
mod sound;
mod system_load;
#[cfg(test)]
pub(crate) mod tests;
mod updater;
mod usage;
mod vcs;
mod worktree_identity;
mod worktree_scripts;
mod wsl_drop;

use std::sync::Arc;

/// An error's message in the current locale, for a window, a notification,
/// or an `$error` placeable. Logs keep `Display`.
pub(crate) trait Localize {
    fn localize(&self) -> String;
}

impl<T: Localize + ?Sized> Localize for &T {
    fn localize(&self) -> String {
        (**self).localize()
    }
}

impl<T: Localize + ?Sized> Localize for Box<T> {
    fn localize(&self) -> String {
        (**self).localize()
    }
}

impl<T: Localize + ?Sized> Localize for Arc<T> {
    fn localize(&self) -> String {
        (**self).localize()
    }
}
