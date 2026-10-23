//! `orchestrator items|issues|prs`: what the orchestrator last synced for a
//! repository, read here from its local database, without Herdr UI and
//! without the network.
use crate::{
    cli::control::Items,
    control::client::Output,
    orchestrator::{self, Item, Repository},
};
use serde::Serialize;
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

/// One synced issue or pull request, as `--json` prints it.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct ItemInfo {
    pub identifier: String,
    pub title: String,
    pub state: String,
    pub url: Option<String>,
    pub author: Option<String>,
    pub labels: Vec<String>,
    /// The pull request's number, when the item is one.
    pub pull_request: Option<u64>,
}

/// The items `which` asks for, most recently updated first.
pub(super) fn select(mut items: Vec<Item>, which: Items) -> Vec<ItemInfo> {
    items.retain(|item| match which {
        Items::All => true,
        Items::Issues => item.pull_request.is_none(),
        Items::PullRequests => item.pull_request.is_some(),
    });
    items.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.identifier.cmp(&b.identifier))
    });
    items
        .into_iter()
        .map(|item| ItemInfo {
            pull_request: item.pull_request.as_ref().map(|pr| pr.number),
            identifier: item.identifier,
            title: item.title,
            state: item.state,
            url: item.url,
            author: item.author,
            labels: item.labels,
        })
        .collect()
}

/// One line per item: its identifier, state, title, author, and address.
pub(super) fn describe(items: &[ItemInfo]) -> String {
    let mut text = String::new();
    for item in items {
        let _ = write!(text, "{}\t{}\t{}", item.identifier, item.state, item.title);
        if let Some(author) = &item.author {
            let _ = write!(text, "\t@{author}");
        }
        if let Some(url) = &item.url {
            let _ = write!(text, "\t{url}");
        }
        text.push('\n');
    }
    text.trim_end().to_owned()
}

/// The repository's git common directory, resolved as the orchestrator
/// keys its database.
pub(super) fn git_common_dir(folder: &Path) -> crate::Result<PathBuf> {
    let output = herdr_client::new_command("git")
        .arg("-C")
        .arg(folder)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .map_err(|source| crate::git::Error::CommonDir {
            path: folder.to_owned(),
            source,
        })?;
    let text = String::from_utf8(output.stdout).ok();
    let common = text
        .as_deref()
        .map(str::trim)
        .filter(|common| output.status.success() && !common.is_empty())
        .ok_or_else(|| crate::Error::NotAGitRepository {
            path: folder.to_owned(),
        })?;
    Ok(
        std::fs::canonicalize(common).map_err(|source| crate::git::Error::CommonDir {
            path: folder.to_owned(),
            source,
        })?,
    )
}

pub(super) fn list(repo: Option<&Path>, which: Items) -> crate::Result<Output> {
    let folder = match repo {
        Some(repo) => repo.to_owned(),
        None => std::env::current_dir()?,
    };
    let common = git_common_dir(&folder)?;
    let path = orchestrator::database_path(&Repository::Local(common)).ok_or(
        crate::Error::OrchestratorItems(orchestrator::Error::NoDataDirectory),
    )?;
    // Opening creates a missing database, which listing must not do.
    if !path.is_file() {
        return Ok(Output::data(
            "The orchestrator has not synced this repository yet; open it with `orchestrator open`",
            &Vec::<ItemInfo>::new(),
        )?
        .nothing());
    }
    let items = orchestrator::Store::open(&path)
        .and_then(|store| store.items())
        .map_err(crate::Error::OrchestratorItems)?;
    let items = select(items, which);
    if items.is_empty() {
        let text = match which {
            Items::All => "No issues or pull requests are synced for this repository",
            Items::Issues => "No issues are synced for this repository",
            Items::PullRequests => "No pull requests are synced for this repository",
        };
        return Output::data(text, &items);
    }
    Output::data(describe(&items), &items)
}
