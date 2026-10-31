//! Shared checks for every error mapping, and a topic file per mapping.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::Localize;
use herdr_i18n::{Locale, Pseudo};
use std::fmt::Display;

mod automations;
mod backdrop;
mod caffeine;
mod chat;
mod chat_protocol;
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
mod enroll;
mod fan_out;
mod file_drop;
mod github;
mod host_build;
mod keymap;
#[cfg(windows)]
mod launch_at_login;
mod link;
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
mod security;
#[cfg(unix)]
mod setup_relay;
mod sound;
mod system_load;
mod terminal_attach;
mod updater;
mod usage;
mod vcs;
mod worktree_identity;
mod worktree_scripts;
mod wsl_drop;

/// Fluent's isolation marks around each placeable, which `Display` lacks.
const ISOLATION: [char; 2] = ['\u{2068}', '\u{2069}'];

/// Each error's English message is its `Display` without isolation marks,
/// so the window says what the log does, and under `en-XA` it is a whole
/// translated message. One call per enum, with every variant.
pub(crate) fn assert_localized<E: Localize + Display>(errors: &[E]) {
    for error in errors {
        crate::i18n::show(&Locale::FALLBACK);
        let english = error.localize().replace(ISOLATION, "");
        assert_eq!(english, error.to_string(), "{error}");
        crate::i18n::show(&Pseudo::Accented.locale());
        let shown = error.localize();
        crate::i18n::show(&Locale::FALLBACK);
        assert!(herdr_i18n::pseudo::is_marked(&shown), "{error}: {shown}");
    }
}

/// Errors whose text is the system's or a parser's, shown as it is in
/// every language.
pub(crate) fn assert_verbatim<E: Localize + Display>(errors: &[E]) {
    for error in errors {
        crate::i18n::show(&Pseudo::Accented.locale());
        let shown = error.localize();
        crate::i18n::show(&Locale::FALLBACK);
        assert_eq!(shown, error.to_string());
    }
}
