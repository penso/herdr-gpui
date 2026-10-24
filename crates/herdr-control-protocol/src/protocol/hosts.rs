//! Hosts and what the app gathers about them: pull requests, listening and
//! forwarded ports, plan usage, load, Dependabot alerts, and automations.
use serde::{Deserialize, Serialize};

/// How long a pull request's comments may take to load.
pub const COMMENTS_WAIT_SECONDS: u64 = 20;
/// How long `ports open` may wait for a new forward to listen.
pub const FORWARD_WAIT_SECONDS: u64 = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostOp {
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
    /// The Dependabot alerts the Security section last read.
    Security,
    /// Show an alert's (`item` a number) or a private report's (`item` a
    /// GHSA ID) page in the Security section.
    SecurityOpen {
        repository: String,
        item: String,
    },
    /// Open the approval of the automation script at `path` on `host` (an
    /// endpoint ID or label; absent, the caller's host). Only the dialog
    /// opens: the user approves there.
    AutomationApprove {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host: Option<String>,
    },
    /// Open the settings that add cloud machines.
    Cloud,
    /// Run the configured setup script again on the cloud device `id`, in a
    /// new tab, showing the device first.
    CloudSetup {
        id: String,
    },
    /// Build the caller's repository's sandbox images, for `provider` (its
    /// name or key) or every provider that holds images.
    CloudImages {
        provider: Option<String>,
    },
}

impl HostOp {
    pub fn wait_seconds(&self) -> u64 {
        match self {
            Self::PrComments => COMMENTS_WAIT_SECONDS,
            Self::OpenPort { .. } => FORWARD_WAIT_SECONDS,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrInfo {
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
pub struct CheckInfo {
    pub name: String,
    pub outcome: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentInfo {
    pub author: String,
    pub kind: String,
    pub created_at: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortInfo {
    pub port: u16,
    pub bind: String,
    pub process: Option<String>,
    /// Where this machine reaches it, once forwarded.
    pub local_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardInfo {
    pub host: String,
    pub port: u16,
    /// "starting", "listening", or "ended".
    pub state: String,
    pub local_url: Option<String>,
    /// Why it ended.
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageInfo {
    pub provider: String,
    pub account: Option<String>,
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageWindow {
    pub label: String,
    pub used_percent: f32,
    pub resets_in_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadInfo {
    pub host: String,
    pub cpu_percent: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub cores: Option<u32>,
    pub load: Option<[f32; 3]>,
    pub uptime_seconds: Option<u64>,
    pub disk_available: Option<u64>,
    pub disk_total: Option<u64>,
    /// Graphics cards that report their usage; absent on hosts without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_count: Option<u32>,
    /// The cards' mean busy share.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_percent: Option<f32>,
    /// Dedicated video memory, summed across cards; absent on Apple GPUs,
    /// which share system memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_memory_used: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_memory_total: Option<u64>,
}

/// A workspace's listening ports, and its host's forwards. Boxed in
/// [`Reply`], which every response carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ports {
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
pub enum Reply {
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
    Security {
        repositories: Vec<SecurityRepoInfo>,
        /// Why the listing may be empty or cut short.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
}

/// One repository the Security section read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityRepoInfo {
    /// `owner/name` on github.com.
    pub name: String,
    pub host: String,
    /// Its Dependabot alerts page.
    pub url: String,
    /// "open", "off" (Dependabot alerts turned off), "hidden" (this sign-in
    /// may not read alerts), or "error".
    pub state: String,
    /// Open alerts GitHub counts, which may be more than `alerts` holds.
    pub total: u64,
    pub alerts: Vec<SecurityAlertInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Private vulnerability reporting: "on", "off", "hidden" (this sign-in
    /// may not read reports), or "error".
    pub reporting: String,
    /// Reports outsiders filed, in triage: titles and metadata only.
    pub reports: Vec<SecurityReportInfo>,
    /// Whether GitHub holds more reports in triage than the section read.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub more_reports: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reports_error: Option<String>,
}

/// A private vulnerability report. Its description is never carried: the
/// vulnerability is not fixed yet, and the user reads it on GitHub.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityReportInfo {
    /// The draft advisory's GHSA identifier.
    pub advisory: String,
    pub summary: String,
    /// The reporter's rating, when they gave one.
    pub severity: Option<String>,
    pub reporter: Option<String>,
    /// RFC 3339.
    pub created_at: Option<String>,
    /// The report's page.
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityAlertInfo {
    pub number: u64,
    /// "critical", "high", "moderate", or "low".
    pub severity: String,
    pub package: String,
    pub ecosystem: String,
    pub summary: String,
    /// The GHSA identifier.
    pub advisory: String,
    pub advisory_url: Option<String>,
    /// The affected version range.
    pub vulnerable: String,
    /// The first fixed version, when one is published.
    pub patched: Option<String>,
    pub manifest: String,
    /// The alert's page.
    pub url: String,
}

#[cfg(test)]
mod tests;
