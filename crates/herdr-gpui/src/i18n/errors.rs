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
#[cfg(feature = "cloud")]
mod cloud_batch;
mod cloud_setup;
mod code_server;
mod config;
mod control;
mod credentials;
mod daemon;
mod dispatch;
mod editor;
mod github;
mod keymap;
mod notification_mutes;
mod orchestrator;
mod protocol;
mod remote;
mod remote_control;
mod root;
#[cfg(test)]
pub(crate) mod tests;
mod updater;
mod usage;
mod vcs;

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

/// Errors not mapped yet show their English `Display` until their own
/// file lands (docs/i18n.md, What is left).
macro_rules! english {
    ($($(#[$meta:meta])* $error:ty),* $(,)?) => {
        $($(#[$meta])*
        impl Localize for $error {
            fn localize(&self) -> String {
                self.to_string()
            }
        })*
    };
}

english! {
    // keep-sorted start
    #[cfg(windows)]
    crate::launch_at_login::Error,
    crate::listening_ports::Error,
    crate::listening_ports::tunnel::Error,
    crate::menu::devices::Error,
    crate::overview::Error,
    crate::palette::Error,
    crate::pane_menu::carry::Refusal,
    crate::pane_move::Error,
    crate::port_forward::Error,
    crate::processes::Error,
    crate::px0::Error,
    crate::review::Error,
    #[cfg(unix)]
    crate::setup_relay::Error,
    crate::sound::Error,
    crate::system_load::Error,
    crate::window::clipboard::Error,
    crate::window::file_drop::Error,
    crate::window::wsl_drop::Error,
    crate::worktree_identity::Error,
    crate::worktree_scripts::Error,
    // keep-sorted end
}
