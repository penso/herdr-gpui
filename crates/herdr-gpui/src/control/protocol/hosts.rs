//! Hosts and what the app gathers about them: pull requests, listening and
//! forwarded ports, plan usage, load, and the copies of their files that
//! link clicks fetched.
use serde::{Deserialize, Serialize};

/// How long a pull request's comments may take to load.
pub(crate) const COMMENTS_WAIT_SECONDS: u64 = 20;
/// How long `ports open` may wait for a new forward to listen.
pub(crate) const FORWARD_WAIT_SECONDS: u64 = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum HostOp {
    /// The window's hosts and their connection status.
    List,
    /// Show `id` in the window, as choosing it in the sidebar does.
    Select {
        id: String,
    },
    /// The pull request of the caller's workspace's branch.
    Pr,
    /// Its review comments, loaded on request.
    PrComments,
    /// Open it in a browser tab.
    PrOpen,
    /// Ports the caller's workspace's processes listen on, and forwards.
    Ports,
    /// Forward a port of the selected SSH host to this machine.
    Forward {
        port: u16,
    },
    Unforward {
        port: u16,
    },
    /// Show `localhost:port` of the caller's workspace in a browser tab,
    /// forwarding or tunnelling a remote port first.
    OpenPort {
        port: u16,
    },
    Usage,
    System,
    /// Open the settings that add cloud machines.
    Cloud,
    /// Run the configured setup script again on the cloud device `id`, in a
    /// new tab, showing the device first.
    CloudSetup {
        id: String,
    },
    /// The copies of SSH hosts' files that link clicks fetched into the
    /// window, and where each stands against its host.
    RemoteFiles,
}

impl HostOp {
    pub(crate) fn wait_seconds(&self) -> u64 {
        match self {
            Self::PrComments => COMMENTS_WAIT_SECONDS,
            Self::OpenPort { .. } => FORWARD_WAIT_SECONDS,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PrInfo {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub state: String,
    pub draft: bool,
    pub head: String,
    pub base: String,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    pub review: Option<String>,
    pub merge_status: Option<String>,
    pub checks: Vec<CheckInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CheckInfo {
    pub name: String,
    pub outcome: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CommentInfo {
    pub author: String,
    pub kind: String,
    pub created_at: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PortInfo {
    pub port: u16,
    pub bind: String,
    pub process: Option<String>,
    /// Where this machine reaches it, once forwarded.
    pub local_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ForwardInfo {
    pub host: String,
    pub port: u16,
    /// "starting", "listening", or "ended".
    pub state: String,
    pub local_url: Option<String>,
    /// Why it ended.
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct UsageInfo {
    pub provider: String,
    pub account: Option<String>,
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct UsageWindow {
    pub label: String,
    pub used_percent: f32,
    pub resets_in_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct LoadInfo {
    pub host: String,
    pub cpu_percent: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub cores: Option<u32>,
    pub load: Option<[f32; 3]>,
    pub uptime_seconds: Option<u64>,
    pub disk_available: Option<u64>,
    pub disk_total: Option<u64>,
}

/// Where a fetched copy stands against its host's file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CopyState {
    /// The copy holds the bytes the host had when last fetched or pushed.
    Synced,
    /// A save waits for the next one: its upload failed.
    Edited,
    /// The app asks, or is about to ask, whether to upload a save.
    Asking,
    Uploading,
    /// The host's file changed since it was synced, so the save was not
    /// uploaded.
    Conflict,
    /// The user chose not to upload this save.
    Declined,
    /// The host's file is being fetched over the copy.
    Refetching,
}

/// A copy of an SSH host's file that a link click fetched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RemoteCopy {
    /// The SSH target.
    pub host: String,
    /// The file's path as the host names it.
    pub path: String,
    /// The copy's path on this machine.
    pub local: String,
    pub state: CopyState,
    /// Whether saves upload without asking.
    pub auto_upload: bool,
}

/// A workspace's listening ports, and its host's forwards. Boxed in
/// [`Reply`], which every response carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ports {
    pub host: String,
    pub workspace: String,
    pub listening: Vec<PortInfo>,
    pub forwards: Vec<ForwardInfo>,
    /// Why the listing may be empty, such as scanning being turned off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Reply {
    Hosts {
        hosts: Vec<super::app::HostInfo>,
    },
    Pr {
        pr: Option<Box<PrInfo>>,
        /// Why there is none to show, when it is not simply that the branch
        /// has no pull request: still looking, or the lookup failed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    Comments {
        comments: Vec<CommentInfo>,
    },
    Ports(Box<Ports>),
    Usage {
        host: String,
        usage: Vec<UsageInfo>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    System {
        load: Option<Box<LoadInfo>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// The copies that fit one answer; `omitted` more did not.
    RemoteFiles {
        copies: Vec<RemoteCopy>,
        #[serde(default)]
        omitted: usize,
    },
}

#[cfg(test)]
mod tests;
