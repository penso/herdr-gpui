//! What Add Device offers when a host cannot be saved as it is, and how it
//! asks: install or update Herdr, continue in a terminal, restart the
//! server, or install the user's SSH key.

use crate::i18n::fl;
use herdr_client::{HostProbe, SshFailure};

/// Why setup has to continue in a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::menu::devices) enum Offer {
    Install,
    Update,
    /// SSH needs a prompt, or saving without a terminal failed.
    Terminal,
    /// Herdr restarts the host's server before saving, once confirmed.
    Restart,
    /// SSH accepted none of the user's keys: install one with the password,
    /// so the device then connects without a prompt.
    InstallKey,
}

impl Offer {
    /// `None` when Herdr is present, so the device is saved without a terminal.
    pub(in crate::menu::devices) fn for_probe(probe: HostProbe) -> Option<Self> {
        match probe {
            HostProbe::Running | HostProbe::Stopped => None,
            HostProbe::Missing => Some(Self::Install),
            HostProbe::Outdated => Some(Self::Update),
            HostProbe::SshFailed(SshFailure::Auth) => Some(Self::InstallKey),
            HostProbe::SshFailed(_) => Some(Self::Terminal),
        }
    }

    /// `user` is the login `ssh` resolved for the target, when known: a target
    /// without one logs in as the local user, which may not be the account
    /// the user's keys are installed for.
    pub(in crate::menu::devices) fn question(self, target: &str, user: Option<&str>) -> String {
        if cfg!(windows) {
            return self.terminal_instructions(target, user);
        }
        match self {
            Self::Install => fl!("add-device-ask-install", target = target),
            Self::Update => fl!("add-device-ask-update", target = target),
            Self::Terminal => fl!("add-device-ask-terminal", target = target),
            Self::Restart => fl!("add-device-ask-restart", target = target),
            Self::InstallKey => match implied_user(target, user) {
                Some(user) => fl!(
                    "add-device-ask-key-user",
                    target = target,
                    user = user,
                    example = example_login(target),
                ),
                None => fl!("add-device-ask-key", target = target),
            },
        }
    }

    pub(super) fn action(self) -> String {
        match self {
            Self::Install => fl!("add-device-install"),
            Self::Update => fl!("add-device-update"),
            Self::Terminal | Self::Restart => fl!("add-device-open-terminal"),
            Self::InstallKey => fl!("add-device-set-up-key"),
        }
    }

    /// Windows cannot run a setup on a local terminal yet (see
    /// `setup::terminal_setup`), so the user runs Herdr's own command in one.
    fn terminal_instructions(self, target: &str, user: Option<&str>) -> String {
        match self {
            Self::Install => fl!("add-device-terminal-install", target = target),
            Self::Update => fl!("add-device-terminal-update", target = target),
            Self::Terminal => fl!("add-device-terminal-input", target = target),
            Self::Restart => fl!("add-device-terminal-restart", target = target),
            Self::InstallKey => match implied_user(target, user) {
                Some(user) => fl!(
                    "add-device-terminal-key-user",
                    target = target,
                    user = user,
                    example = example_login(target),
                ),
                None => fl!("add-device-terminal-key", target = target),
            },
        }
    }
}

/// `target` with a placeholder login, to show how to name another account.
fn example_login(target: &str) -> String {
    with_user(target, &fl!("add-device-example-user"))
}

/// The resolved login, only when `target` does not name one itself: the
/// dialog then has not shown which account SSH tried.
fn implied_user<'a>(target: &str, user: Option<&'a str>) -> Option<&'a str> {
    let named = target
        .strip_prefix("ssh://")
        .unwrap_or(target)
        .contains('@');
    user.filter(|_| !named)
}

/// `target` logging in as `user`; an `ssh://` URI takes the user inside it.
fn with_user(target: &str, user: &str) -> String {
    match target.strip_prefix("ssh://") {
        Some(rest) => format!("ssh://{user}@{rest}"),
        None => format!("{user}@{target}"),
    }
}
