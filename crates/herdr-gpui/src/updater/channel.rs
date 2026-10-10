//! Which published releases the updater offers. Defined here, not in
//! `config`, because scripts/test-updater.py compiles the updater on its own.
use serde::Deserialize;

/// Betas are GitHub prereleases, published from `main` like any release and
/// later promoted to stable in place. Only the stable channel reaches
/// Homebrew; the beta channel offers the newest of both.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

impl UpdateChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }
}
