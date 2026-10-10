//! `[updates]`: which published releases the in-app updater offers.
pub use crate::updater::UpdateChannel;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UpdatesConfig {
    pub channel: UpdateChannel,
}
