//! Bounded presentation data from Herdr UI's existing PR lookup cache.
pub use herdr_github::pull_request::Tone;
use serde::{Deserialize, Serialize};

pub const MAX_PULL_REQUESTS: usize = 128;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PullRequests {
    pub boot_id: String,
    pub entries: Vec<WorkspacePullRequest>,
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
    }
}

impl WorkspacePullRequest {
    pub fn valid(&self) -> bool {
        self.number > 0
            && !self.workspace_id.is_empty()
            && self.workspace_id.len() <= 128
            && !self.cwd.is_empty()
            && self.cwd.len() <= 4096
            && !self.branch.is_empty()
            && self.branch.len() <= 512
            && self.url.len() <= 2048
            && url::Url::parse(&self.url).is_ok_and(|url| {
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
            })
    }
}
