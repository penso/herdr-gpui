//! A UTF-8 locale for children started without one. launchd gives an app
//! opened from the Dock, Finder, or at login no `LANG`, and a macOS login
//! shell rarely exports one, because terminals such as Terminal, iTerm2, and
//! Ghostty set it themselves. Without it the daemon and every pane run in the
//! C locale, where `pbcopy` reads UTF-8 as Mac Roman: `für` copied from a
//! program in a pane, such as Claude Code, pastes as `f√ºr`. Linux desktop
//! sessions usually set `LANG` (systemd defaults to `C.UTF-8`), so there this
//! only covers launches without one.
//!
//! Windows needs nothing: native programs take their encoding from the system
//! and console code pages, and Cygwin and MSYS2 default to `C.UTF-8` when
//! `LANG` is unset.

use std::{ffi::OsString, process::Command};

/// What a child gets when no variable chooses its encoding. macOS has
/// `en_US.UTF-8` on every supported release, and Ghostty falls back to it too;
/// besides the encoding it sets English collation and number grouping, as the
/// region-derived locales terminals set do. Elsewhere `C.UTF-8` changes only
/// the encoding: musl builds it in, and glibc has shipped it since 2.35 for
/// distributions to install, as Ubuntu and Fedora do, while `en_US.UTF-8`
/// exists only where someone generated it.
#[cfg(target_os = "macos")]
const FALLBACK: &str = "en_US.UTF-8";
#[cfg(not(target_os = "macos"))]
const FALLBACK: &str = "C.UTF-8";

/// The variables that choose the character encoding, `LC_ALL` first.
const ENCODING_VARIABLES: [&str; 3] = ["LC_ALL", "LC_CTYPE", "LANG"];

/// Gives `command` `LANG=`[`FALLBACK`] when no encoding variable is set, on
/// the command or in `inherited`, this process's environment. A locale someone
/// did choose, even `C`, is kept, and an empty value counts as unset, as POSIX
/// specifies. `command` must inherit this process's environment: after
/// `env_clear`, a variable removed from it no longer shows in `get_envs`.
pub(super) fn fill_in(command: &mut Command, inherited: impl Fn(&str) -> Option<OsString>) {
    let chosen = |name: &str| match command.get_envs().find(|(key, _)| *key == name) {
        Some((_, value)) => value.is_some_and(|value| !value.is_empty()),
        None => inherited(name).is_some_and(|value| !value.is_empty()),
    };
    if !ENCODING_VARIABLES.into_iter().any(chosen) {
        command.env("LANG", FALLBACK);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
