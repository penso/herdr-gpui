//! The app's saved tabs, as anything other than the app reads them. Only
//! tabs of herdr-core terminals matter here, so every other kind of tab, and
//! one that fails to read, is skipped rather than costing the rest.

mod error;

pub use error::Error;

use crate::panes::Panes;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// The largest saved tabs file read.
pub const MAX_SAVED_BYTES: usize = 1024 * 1024;
/// The most saved tabs read.
pub const MAX_SAVED_TABS: usize = 256;

#[derive(Debug, Deserialize)]
pub struct SavedTabs {
    tabs: Vec<SavedTab>,
}

#[derive(Debug, Deserialize)]
struct SavedTab {
    id: u64,
    #[serde(default, deserialize_with = "core_location")]
    location: Option<CoreLocation>,
    scope: String,
    title: String,
    workspace_id: String,
}

/// The `core_terminal` kind of the app's `Location`. An enum because only
/// an enum's tag is checked: any other kind fails, and reads as none.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CoreLocation {
    CoreTerminal {
        #[serde(default)]
        observe: bool,
        #[serde(alias = "terminal")]
        panes: Panes,
    },
}

fn core_location<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<CoreLocation>, D::Error> {
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok()))
}

/// One tab of herdr-core terminals that types into them.
#[derive(Clone, Copy, Debug)]
pub struct CoreTab<'a> {
    pub id: u64,
    pub panes: &'a Panes,
    pub title: &'a str,
    pub workspace_id: &'a str,
}

impl SavedTabs {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_SAVED_BYTES {
            return Err(Error::Large);
        }
        let saved: Self = serde_json::from_slice(bytes)?;
        if saved.tabs.len() > MAX_SAVED_TABS {
            return Err(Error::Many);
        }
        Ok(saved)
    }

    /// `scope`'s tabs in a workspace, in saved order. Mirrors are left out:
    /// the terminal they watch is listed where it types.
    pub fn core_tabs<'a>(&'a self, scope: &'a str) -> impl Iterator<Item = CoreTab<'a>> {
        self.tabs
            .iter()
            .filter(move |tab| tab.scope == scope && !tab.workspace_id.is_empty())
            .filter_map(|tab| {
                let CoreLocation::CoreTerminal { observe, panes } = tab.location.as_ref()?;
                (!observe).then_some(CoreTab {
                    id: tab.id,
                    panes,
                    title: &tab.title,
                    workspace_id: &tab.workspace_id,
                })
            })
    }
}
