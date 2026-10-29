//! Records in the shapes the shared database stores them. JSON columns use
//! agent-launcher's serde field names, so either application decodes the
//! other's rows.

use super::{Error, Result};
use chrono::{DateTime, Utc};
use herdr_client::protocol::AgentStatus;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, str::FromStr};

/// Where an item comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Provider {
    Github,
    Gitlab,
    Beads,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitlab => "gitlab",
            Self::Beads => "beads",
        }
    }
}

impl FromStr for Provider {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "github" => Ok(Self::Github),
            "gitlab" => Ok(Self::Gitlab),
            "beads" => Ok(Self::Beads),
            other => Err(Error::Provider(other.to_owned())),
        }
    }
}

/// Escapes one key component so joined keys stay injective.
fn component(value: &str) -> String {
    value.replace('%', "%25").replace(':', "%3A")
}

/// Reverses [`component`]: `%3A` first, so an escaped `%253A` stays `%3A`.
fn uncomponent(value: &str) -> String {
    value.replace("%3A", ":").replace("%25", "%")
}

/// One synced source: a provider's view of one repository.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SourceKey {
    pub provider: Provider,
    pub host: String,
    pub repository: String,
}

impl SourceKey {
    /// `provider:host:repository`, the `source` column of issues and
    /// checkpoints.
    pub fn canonical(&self) -> String {
        format!(
            "{}:{}:{}",
            self.provider.as_str(),
            component(&self.host),
            component(&self.repository)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ItemKey {
    pub source: SourceKey,
    /// An issue's number, `pr/<number>` for a GitHub pull request or GitLab
    /// merge request, or a Beads id.
    pub native_id: String,
}

impl ItemKey {
    pub fn canonical(&self) -> String {
        format!("{}:{}", self.source.canonical(), component(&self.native_id))
    }
}

/// Parses [`ItemKey::canonical`]: escaped components never hold a `:`.
impl FromStr for ItemKey {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let parts: Vec<&str> = value.split(':').collect();
        let [provider, host, repository, native_id] = parts[..] else {
            return Err(Error::ItemKey(value.to_owned()));
        };
        if native_id.is_empty() {
            return Err(Error::ItemKey(value.to_owned()));
        }
        Ok(Self {
            source: SourceKey {
                provider: provider.parse()?,
                host: uncomponent(host),
                repository: uncomponent(repository),
            },
            native_id: uncomponent(native_id),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub base_ref: String,
    pub head_ref: String,
    pub base_sha: String,
    pub head_sha: String,
    /// The repository holding the head branch, as `owner/name`.
    pub head_repository: Option<String>,
}

impl Item {
    /// What a run's page shows when its item is no longer listed, such as
    /// an issue closed since: the key alone, so the run keeps its controls.
    pub fn stand_in(key: ItemKey) -> Self {
        let number = key.native_id.strip_prefix("pr/").unwrap_or(&key.native_id);
        let identifier = match key.source.provider {
            Provider::Beads => key.native_id.clone(),
            // GitLab calls a merge request `!iid`; its issues stay `#iid`.
            Provider::Gitlab if key.native_id.starts_with("pr/") => format!("!{number}"),
            _ => format!("#{number}"),
        };
        Self {
            key,
            identifier,
            title: "No longer listed".into(),
            description: None,
            state: "unlisted".into(),
            url: None,
            author: None,
            labels: Vec::new(),
            parent_id: None,
            blocked_by: Vec::new(),
            priority: None,
            created_at: None,
            updated_at: None,
            pull_request: None,
            activity: None,
        }
    }
}

impl Item {
    /// The pull request, when it is GitHub's: the only kind whose status,
    /// conversation, and merge the app reads and acts on, and whose review
    /// prompt `gh` serves.
    pub fn github_pull_request(&self) -> Option<&PullRequest> {
        self.pull_request
            .as_ref()
            .filter(|_| self.key.source.provider == Provider::Github)
    }
}

/// Provider-reported engagement counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    pub comments: Option<u64>,
    pub review_comments: Option<u64>,
    pub commits: Option<u64>,
}

/// An issue, pull request, or bead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub key: ItemKey,
    /// What the user reads: `#377`, or a Beads id.
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    /// The provider's own state word, such as `open`, `merged`, or `in_progress`.
    pub state: String,
    pub url: Option<String>,
    pub author: Option<String>,
    pub labels: Vec<String>,
    pub parent_id: Option<String>,
    pub blocked_by: Vec<String>,
    pub priority: Option<i64>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub pull_request: Option<PullRequest>,
    pub activity: Option<Activity>,
}

/// A source's sync position. `pr_details` holds agent-launcher's opaque PR
/// revision markers; herdr-gpui keeps the ones it finds and adds none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub updated_at: Option<DateTime<Utc>>,
    pub etag: Option<String>,
    pub last_full_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub pr_details: HashMap<String, String>,
    #[serde(default)]
    pub pr_cursor: Option<u64>,
}

/// Which application dispatched a run, and so alone may write it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    AgentLauncher,
    HerdrGpui,
    /// No application: a run found from its item's branch, never stored.
    Branch,
}

impl Owner {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentLauncher => "agent-launcher",
            Self::HerdrGpui => "herdr-gpui",
            Self::Branch => "branch",
        }
    }
}

impl FromStr for Owner {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "agent-launcher" => Ok(Self::AgentLauncher),
            "herdr-gpui" => Ok(Self::HerdrGpui),
            other => Err(Error::Owner(other.to_owned())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Provisioning,
    Starting,
    Running,
    NeedsInput,
    Idle,
    Completed,
    Failed,
    Cancelled,
    Disconnected,
}

/// The agent-launcher backend a run went through; herdr-gpui only uses Herdr.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Superset,
    Native,
    Herdr,
    Conductor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub backend: Backend,
    pub id: String,
    pub host: Option<String>,
    pub path: Option<PathBuf>,
    pub branch: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub id: String,
    /// The canonical key of the item the run works on.
    pub item_key: String,
    pub workspace: Option<Workspace>,
    pub agent: String,
    pub model: Option<String>,
    pub state: RunState,
    pub message: Option<String>,
    pub session_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub owner: Owner,
}

impl Run {
    /// When the run began, unknown for one found from its branch.
    pub fn started(&self) -> Option<DateTime<Utc>> {
        (self.owner != Owner::Branch).then_some(self.started_at)
    }
}

/// Where a run's agent lives in Herdr, written by the run's owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HerdrSession {
    pub run_id: String,
    /// `None` on the machine that owns the database, else an SSH destination.
    pub host: Option<String>,
    /// The Herdr `--session` name, `None` for the default session.
    pub session: Option<String>,
    pub workspace_id: String,
    pub pane_id: String,
    pub agent_name: String,
    pub updated_at: DateTime<Utc>,
}

/// An agent Herdr shows on one of the client's connected hosts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveAgent {
    /// The endpoint it is on, by index in the window's list.
    pub endpoint: usize,
    /// `None` on this machine, else the SSH destination.
    pub host: Option<String>,
    pub session: Option<String>,
    pub workspace_id: String,
    pub pane_id: String,
    pub status: AgentStatus,
    /// The agent's own summary of what it is doing, if it gave one.
    pub title: Option<String>,
}
