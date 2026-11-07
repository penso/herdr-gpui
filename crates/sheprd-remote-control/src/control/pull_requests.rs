//! Bounded presentation data from Sheprd's existing PR lookup cache.
use serde::{Deserialize, Serialize};
pub use sheprd_github::pull_request::Tone;

pub const MAX_PULL_REQUESTS: usize = 128;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PullRequests {
    pub boot_id: String,
    pub entries: Vec<WorkspacePullRequest>,
    /// Workspaces whose commits no pull request carries yet. An older app
    /// sends none.
    #[serde(default)]
    pub unopened: Vec<UnopenedBranch>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspacePullRequest {
    pub workspace_id: String,
    pub cwd: String,
    pub branch: String,
    pub number: u64,
    pub url: String,
    pub tone: Tone,
}

/// A workspace's branch with commits past its default branch and, by a
/// finished lookup, no pull request carrying them.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnopenedBranch {
    pub workspace_id: String,
    pub cwd: String,
    pub branch: String,
    pub commits: u64,
}

impl PullRequests {
    pub fn valid(&self) -> bool {
        !self.boot_id.is_empty()
            && self.boot_id.len() <= 128
            && self.entries.len() <= MAX_PULL_REQUESTS
            && self.entries.iter().all(WorkspacePullRequest::valid)
            && self
                .entries
                .iter()
                .map(|entry| &entry.workspace_id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                == self.entries.len()
            && self.unopened.len() <= MAX_PULL_REQUESTS
            && self.unopened.iter().all(UnopenedBranch::valid)
            && self
                .unopened
                .iter()
                .map(|entry| &entry.workspace_id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                == self.unopened.len()
    }
}

/// The bounds both kinds of entry put on the workspace they name.
fn valid_place(workspace_id: &str, cwd: &str, branch: &str) -> bool {
    !workspace_id.is_empty()
        && workspace_id.len() <= 128
        && !cwd.is_empty()
        && cwd.len() <= 4096
        && !branch.is_empty()
        && branch.len() <= 512
}

impl UnopenedBranch {
    pub fn valid(&self) -> bool {
        self.commits > 0 && valid_place(&self.workspace_id, &self.cwd, &self.branch)
    }
}

impl WorkspacePullRequest {
    pub fn valid(&self) -> bool {
        self.number > 0
            && valid_place(&self.workspace_id, &self.cwd, &self.branch)
            && self.url.len() <= 2048
            && url::Url::parse(&self.url).is_ok_and(|url| {
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
            })
    }
}
