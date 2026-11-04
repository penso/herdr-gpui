//! A GitLab merge request in the shared pull request shape, so the Pull
//! requests list shows it beside GitHub's.

use super::{Author, Listed};
use crate::{Activity, Item, ItemKey, PullRequest, SourceKey};
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// One entry of `GET /projects/:id/merge_requests`.
#[derive(Deserialize)]
pub struct MergeRequestNode {
    iid: u64,
    title: String,
    description: Option<String>,
    state: String,
    #[serde(default)]
    draft: bool,
    /// What GitLab before 14.0 calls a draft; newer ones send both.
    #[serde(default)]
    work_in_progress: bool,
    web_url: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    author: Option<Author>,
    #[serde(default)]
    labels: Vec<String>,
    user_notes_count: Option<u64>,
    source_branch: String,
    target_branch: String,
    /// The head commit of the source branch.
    sha: Option<String>,
    diff_refs: Option<DiffRefs>,
}

#[derive(Deserialize)]
struct DiffRefs {
    base_sha: Option<String>,
    head_sha: Option<String>,
}

impl MergeRequestNode {
    /// The state word the list filters and colors by: GitLab's `opened`
    /// reads as `open`, or `draft`; `locked`, a merge in progress, as `open`.
    fn state(&self) -> &'static str {
        match self.state.as_str() {
            "merged" => "merged",
            "closed" => "closed",
            _ if self.draft || self.work_in_progress => "draft",
            _ => "open",
        }
    }
}

impl Listed for MergeRequestNode {
    const COLLECTION: &'static str = "merge_requests";

    fn iid(&self) -> u64 {
        self.iid
    }

    fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    fn into_item(self, source: &SourceKey) -> Item {
        let state = self.state();
        let refs = self.diff_refs;
        Item {
            key: ItemKey {
                source: source.clone(),
                native_id: format!("pr/{}", self.iid),
            },
            identifier: format!("!{}", self.iid),
            title: self.title,
            description: self.description.filter(|body| !body.is_empty()),
            state: state.to_owned(),
            url: Some(self.web_url),
            author: self.author.map(|author| author.username),
            labels: self.labels,
            parent_id: None,
            blocked_by: Vec::new(),
            priority: None,
            created_at: Some(self.created_at),
            updated_at: Some(self.updated_at),
            pull_request: Some(PullRequest {
                number: self.iid,
                // The listing carries no diff size.
                additions: None,
                deletions: None,
                base_ref: self.target_branch,
                head_ref: self.source_branch,
                base_sha: refs
                    .as_ref()
                    .and_then(|refs| refs.base_sha.clone())
                    .unwrap_or_default(),
                head_sha: self
                    .sha
                    .or_else(|| refs.and_then(|refs| refs.head_sha))
                    .unwrap_or_default(),
                // A fork is a project id here, which names no repository.
                head_repository: None,
            }),
            activity: Some(Activity {
                comments: self.user_notes_count,
                ..Activity::default()
            }),
        }
    }
}
