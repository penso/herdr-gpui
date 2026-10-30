#![allow(clippy::unwrap_used)]

use super::{ActivityPanel, BAR_WIDTH};
use crate::{
    HerdrWindow,
    orchestrator::{Item, ItemKey, Provider, SourceKey},
};

mod back;
mod bubbles;
mod counts;
mod layout;
mod mod_apps;
mod sections;
mod startup_counts;
mod toggle;

fn item(native_id: &str, title: &str) -> Item {
    Item {
        key: ItemKey {
            source: SourceKey {
                provider: Provider::Github,
                host: "github.com".into(),
                repository: "penso/herdr-gpui".into(),
            },
            native_id: native_id.into(),
        },
        identifier: format!("#{native_id}"),
        title: title.into(),
        description: None,
        state: "open".into(),
        url: None,
        author: None,
        labels: vec!["enhancement".into()],
        parent_id: None,
        blocked_by: Vec::new(),
        priority: None,
        created_at: None,
        updated_at: None,
        pull_request: None,
        activity: None,
    }
}

fn pull_request(number: u64) -> crate::orchestrator::PullRequest {
    crate::orchestrator::PullRequest {
        number,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: format!("b-{number}"),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    }
}
