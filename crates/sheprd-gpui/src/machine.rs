//! Machine tabs: a virtual Mac in a tab, as a browser tab shows a page.
//!
//! The virtual Mac runs inside the app, so it stops with the app, as a page
//! does. Its picture is Apple's own view, which only works in the process
//! that runs the machine. That view takes a page's place in the window
//! ([`crate::browser::page`]), so the window's menus and toasts draw over it.
//! Only Apple silicon Macs can run one; elsewhere a saved tab says so.
//!
//! A machine outlives its tab: closing the tab leaves it running, which the
//! status bar shows. Quitting the app saves it, and it resumes when next shown.

#[cfg(target_os = "macos")]
mod configuration;
mod error;
#[cfg(target_os = "macos")]
mod quit;
#[cfg(target_os = "macos")]
mod settings;
#[cfg(target_os = "macos")]
mod store;
#[cfg(target_os = "macos")]
mod vm;

pub use error::{Error, Result};
#[cfg(target_os = "macos")]
pub(crate) use {
    settings::{Display, Limits, Settings},
    store::{Listing, SettingsQueue, discard_state, list, save_settings, update_settings},
    vm::{Status, delete, release, running, shut_down, start, status, suspend, view},
};

use serde::{Deserialize, Serialize};
use std::fmt;

/// The longest name a machine takes.
const MAX_NAME_BYTES: usize = 64;

/// A machine's name, which is the name of its folder. It comes from the
/// command line and from saved tabs, so it is checked whenever it is read.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct Name(String);

impl Name {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Name {
    type Error = Error;

    /// ASCII letters, digits, `-`, and `_`, starting with a letter or digit,
    /// so a name can neither leave the machines' folder nor look like an
    /// option.
    fn try_from(name: String) -> Result<Self> {
        let valid = !name.is_empty()
            && name.len() <= MAX_NAME_BYTES
            && name.starts_with(|first: char| first.is_ascii_alphanumeric())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if valid {
            Ok(Self(name))
        } else {
            Err(Error::InvalidName)
        }
    }
}

impl From<Name> for String {
    fn from(name: Name) -> Self {
        name.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests;
