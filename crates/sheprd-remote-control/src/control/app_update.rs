//! Updating Sheprd from a phone. The daemon only relays: the app reports
//! where its updater stands with each poll, and that same exchange hands it
//! a phone's request. The app's own updater then checks, verifies the
//! signed release, installs, and restarts, and the new app restarts this
//! daemon from the binary beside it, so a phone never names what to install.

use serde::{Deserialize, Serialize};

/// The longest version or failure text a phone is shown.
pub const MAX_UPDATE_TEXT: usize = 512;

/// Where the app's updater stands, as a phone shows it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    // keep-sorted start
    /// A newer release can be installed.
    Available,
    /// Working: checking, downloading, installing, or restarting.
    Busy,
    /// Nothing newer was found.
    Current,
    /// This build does not update itself; `detail` says why.
    Disabled,
    /// The last attempt failed; `detail` says why.
    Failed,
    /// Not checked yet.
    Idle,
    // keep-sorted end
}

/// What the app says about its updater.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppUpdate {
    pub stage: Stage,
    /// The newest release known, when one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AppUpdate {
    /// Whether every text fits what a phone is shown.
    pub fn valid(&self) -> bool {
        [&self.latest, &self.detail]
            .into_iter()
            .flatten()
            .all(|text| text.len() <= MAX_UPDATE_TEXT)
    }
}
