//! The app's own tabs: pages, code, reviews, the orchestrator, and the
//! Devices overview, which Herdr does not know about.
use serde::{Deserialize, Serialize};

/// How long a page may take to hand back its text or picture.
pub(crate) const PAGE_WAIT_SECONDS: u64 = 15;
/// How long the window may take to switch to the caller's workspace before
/// opening a tab there.
pub(crate) const ARRIVE_WAIT_SECONDS: u64 = 5;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum TabOp {
    /// Every app tab, or only browser pages.
    List {
        #[serde(default)]
        pages: bool,
    },
    Focus {
        id: u64,
    },
    Close {
        id: u64,
    },
    /// Load a web address in an existing page tab.
    Navigate {
        id: u64,
        url: String,
    },
    Back {
        id: u64,
    },
    Forward {
        id: u64,
    },
    /// The page's visible text, as the user would select it.
    Text {
        id: u64,
    },
    /// A PNG of the page, written to `path`.
    Screenshot {
        id: u64,
        path: String,
    },
    /// How large a page tab draws its page.
    Zoom {
        id: u64,
        change: ZoomChange,
    },
    /// A read-only code tab on a local file, at a 1-based line.
    Code {
        path: String,
        #[serde(default)]
        line: Option<u32>,
    },
    /// The file in the user's terminal editor, in a new pane.
    Edit {
        path: String,
        #[serde(default)]
        line: Option<u32>,
    },
    Devices,
    /// The orchestrator of the caller's workspace, or of the checkout at
    /// `repo`.
    Orchestrator {
        #[serde(default)]
        repo: Option<String>,
    },
    /// The review of the caller's workspace's uncommitted changes.
    Review,
}

impl TabOp {
    pub(crate) fn wait_seconds(&self) -> u64 {
        match self {
            Self::Text { .. } | Self::Screenshot { .. } => PAGE_WAIT_SECONDS,
            Self::Code { .. }
            | Self::Edit { .. }
            | Self::Devices
            | Self::Orchestrator { .. }
            | Self::Review => ARRIVE_WAIT_SECONDS,
            Self::List { .. }
            | Self::Focus { .. }
            | Self::Close { .. }
            | Self::Navigate { .. }
            | Self::Back { .. }
            | Self::Forward { .. }
            | Self::Zoom { .. } => 0,
        }
    }
}

/// A step in or out, back to 100%, or a zoom of its own, in percent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case", deny_unknown_fields)]
pub enum ZoomChange {
    In,
    Out,
    Reset,
    Percent { percent: u16 },
}

impl TabKind {
    /// The kind as `--json` spells it, for the text listing.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::Web => "web",
            Self::File => "file",
            Self::Code => "code",
            Self::Review => "review",
            Self::Orchestrator => "orchestrator",
            Self::Devices => "devices",
            Self::Editor => "editor",
        }
    }

    /// Whether a page shows it: what `browser list` keeps.
    pub(crate) fn is_page(self) -> bool {
        matches!(self, Self::Web | Self::File)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TabKind {
    Blank,
    Web,
    File,
    Code,
    Review,
    Orchestrator,
    Devices,
    /// The workspace's VS Code tab.
    Editor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TabInfo {
    pub id: u64,
    pub kind: TabKind,
    pub title: String,
    /// The address, file, or checkout the tab shows.
    pub location: Option<String>,
    pub workspace: String,
    /// The host the workspace belongs to: a local socket or an SSH host.
    pub host: String,
    /// The pane that opened the tab, when an agent did.
    pub opened_by: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Reply {
    Tabs {
        tabs: Vec<TabInfo>,
    },
    Opened {
        id: u64,
        workspace: String,
    },
    Text {
        text: String,
    },
    /// The zoom a page tab draws at now, in percent.
    Zoom {
        id: u64,
        percent: u16,
    },
}

#[cfg(test)]
mod tests;
