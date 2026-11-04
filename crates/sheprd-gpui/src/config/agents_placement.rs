//! Where the sidebar lists agents while `show_agents` is on.
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentsPlacement {
    /// Their own section below the workspaces, every host's agents in one
    /// order, with a split between the two.
    #[default]
    Panel,
    /// Under the workspace each one runs in, so the workspaces take the
    /// sidebar's full height.
    Nested,
}

impl AgentsPlacement {
    /// The value written to the config file; matches the serde name.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Panel => "panel",
            Self::Nested => "nested",
        }
    }
}
