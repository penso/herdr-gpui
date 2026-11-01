//! An Azure Repos pull request in the shared pull request shape, so the Pull
//! requests list shows it beside GitHub's and GitLab's.

use super::Identity;
use crate::{Item, ItemKey, PullRequest, SourceKey};
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// The reply of `GET .../pullrequests`.
#[derive(Deserialize)]
pub struct Page {
    pub value: Vec<PullRequestNode>,
}

/// One entry of `GET {project}/_apis/git/repositories/{repo}/pullrequests`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestNode {
    pull_request_id: u64,
    title: String,
    description: Option<String>,
    /// `active`, `completed`, or `abandoned`.
    status: String,
    #[serde(default)]
    is_draft: bool,
    creation_date: DateTime<Utc>,
    created_by: Option<Identity>,
    source_ref_name: String,
    target_ref_name: String,
    last_merge_source_commit: Option<Commit>,
    last_merge_target_commit: Option<Commit>,
    #[serde(default)]
    labels: Vec<Label>,
    /// Named in an organization-wide listing, which mixes repositories.
    repository: Option<RepositoryRef>,
}

#[derive(Deserialize)]
struct RepositoryRef {
    name: String,
    project: Option<ProjectRef>,
}

#[derive(Deserialize)]
struct ProjectRef {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    commit_id: String,
}

#[derive(Deserialize)]
struct Label {
    name: String,
}

/// A ref as the list shows a branch: `refs/heads/main` reads `main`.
fn branch(name: String) -> String {
    match name.strip_prefix("refs/heads/") {
        Some(branch) => branch.to_owned(),
        None => name,
    }
}

impl PullRequestNode {
    pub fn id(&self) -> u64 {
        self.pull_request_id
    }

    /// The repository of `organization` it belongs to, as a source, when the
    /// listing names it: an organization-wide listing mixes repositories,
    /// and each pull request is keyed by its own so it is the row a listing
    /// of that repository shows.
    pub fn source_in(&self, organization: &str) -> Option<SourceKey> {
        let repository = self.repository.as_ref()?;
        let project = repository.project.as_ref()?;
        [
            organization,
            project.name.as_str(),
            repository.name.as_str(),
        ]
        .iter()
        .all(|part| !part.is_empty() && !part.contains('/'))
        .then(|| SourceKey {
            provider: crate::Provider::Azure,
            host: super::HOST.to_owned(),
            repository: format!("{organization}/{}/{}", project.name, repository.name),
        })
    }

    /// The state word the list filters and colors by.
    fn state(&self) -> &'static str {
        match self.status.as_str() {
            "completed" => "merged",
            "abandoned" => "closed",
            _ if self.is_draft => "draft",
            _ => "open",
        }
    }

    /// `web` is the repository's address, which the pull request's page
    /// hangs from; the listing itself carries only API addresses.
    pub fn into_item(self, source: &SourceKey, web: &str) -> Item {
        let state = self.state();
        let id = self.pull_request_id;
        let url = Some(format!("{}/pullrequest/{id}", web.trim_end_matches('/')));
        Item {
            key: ItemKey {
                source: source.clone(),
                native_id: format!("pr/{id}"),
            },
            identifier: format!("!{id}"),
            title: self.title,
            description: self.description.filter(|body| !body.is_empty()),
            state: state.to_owned(),
            url,
            author: self.created_by.and_then(Identity::name),
            labels: self.labels.into_iter().map(|label| label.name).collect(),
            parent_id: None,
            blocked_by: Vec::new(),
            priority: None,
            created_at: Some(self.creation_date),
            // The listing carries no update time.
            updated_at: Some(self.creation_date),
            pull_request: Some(PullRequest {
                number: id,
                additions: None,
                deletions: None,
                base_ref: branch(self.target_ref_name),
                head_ref: branch(self.source_ref_name),
                base_sha: self
                    .last_merge_target_commit
                    .map(|commit| commit.commit_id)
                    .unwrap_or_default(),
                head_sha: self
                    .last_merge_source_commit
                    .map(|commit| commit.commit_id)
                    .unwrap_or_default(),
                // A fork is another repository id here, which names no path.
                head_repository: None,
            }),
            activity: None,
        }
    }
}
