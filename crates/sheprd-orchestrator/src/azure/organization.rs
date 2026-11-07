//! What an Azure DevOps organization holds beyond one checkout: its projects
//! and their repositories, to browse, and every pull request the reader
//! created in any of them, in every state.

use super::{API_VERSION, Call, Error, HOST, PAGE, Repository, parse, pull_request, reader};
use crate::Item;
use serde::Deserialize;
use std::collections::HashSet;

/// The most repositories a catalog keeps; an organization with more lists
/// the first ones by project and name.
pub const MAX_REPOSITORIES: usize = 2_000;
/// The most of the reader's own pull requests one read keeps, newest first.
pub const MAX_MINE: usize = 500;

/// An organization's projects, each with its repositories, sorted by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub organization: String,
    pub projects: Vec<Project>,
    /// More repositories than [`MAX_REPOSITORIES`] were listed.
    pub capped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub repositories: Vec<String>,
}

/// The reply of `GET {organization}/_apis/git/repositories`.
#[derive(Deserialize)]
struct RepositoryList {
    value: Vec<RepositoryEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryEntry {
    name: String,
    project: Option<Named>,
    #[serde(default)]
    is_disabled: bool,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

/// `https://dev.azure.com/{organization}/` followed by `segments`, each
/// escaped as one path segment.
pub fn url(organization: &str, segments: &[&str]) -> Result<url::Url, Error> {
    let invalid = || Error::Address(organization.to_owned());
    if organization.is_empty() || organization.contains('/') {
        return Err(invalid());
    }
    let mut url = url::Url::parse(&format!("https://{HOST}")).map_err(|_| invalid())?;
    url.path_segments_mut()
        .map_err(|()| invalid())?
        .push(organization)
        .extend(segments);
    Ok(url)
}

/// Every Git repository of `organization`, across its projects.
pub fn repositories_url(organization: &str) -> Result<url::Url, Error> {
    let mut url = url(organization, &["_apis", "git", "repositories"])?;
    url.query_pairs_mut()
        .append_pair("api-version", API_VERSION);
    Ok(url)
}

/// The pull requests `creator` created in `organization`, in every state,
/// newest first, `skip` of them passed.
pub fn mine_url(organization: &str, creator: &str, skip: usize) -> Result<url::Url, Error> {
    let mut url = url(organization, &["_apis", "git", "pullrequests"])?;
    url.query_pairs_mut()
        .append_pair("searchCriteria.creatorId", creator)
        .append_pair("searchCriteria.status", "all")
        .append_pair("$top", &PAGE.to_string())
        .append_pair("$skip", &skip.to_string())
        .append_pair("api-version", API_VERSION);
    Ok(url)
}

/// Lists `organization`'s projects and their enabled repositories.
pub fn catalog(
    organization: &str,
    fetch: &mut impl FnMut(Call) -> crate::Result<Vec<u8>>,
) -> crate::Result<Catalog> {
    let list: RepositoryList = parse(&fetch(Call::Get(repositories_url(organization)?))?)?;
    let mut entries: Vec<(String, String)> = list
        .value
        .into_iter()
        .filter(|entry| !entry.is_disabled && !entry.name.is_empty())
        .filter_map(|entry| Some((entry.project?.name, entry.name)))
        .filter(|(project, _)| !project.is_empty())
        .collect();
    let by_name = |text: &str| text.to_lowercase();
    entries.sort_by(|a, b| {
        by_name(&a.0)
            .cmp(&by_name(&b.0))
            .then_with(|| by_name(&a.1).cmp(&by_name(&b.1)))
    });
    entries.dedup();
    let capped = entries.len() > MAX_REPOSITORIES;
    entries.truncate(MAX_REPOSITORIES);
    let mut projects: Vec<Project> = Vec::new();
    for (project, repository) in entries {
        match projects.last_mut() {
            Some(last) if last.name == project => last.repositories.push(repository),
            _ => projects.push(Project {
                name: project,
                repositories: vec![repository],
            }),
        }
    }
    Ok(Catalog {
        organization: organization.to_owned(),
        projects,
        capped,
    })
}

/// The reader's own pull requests across an organization.
pub struct Mine {
    /// Each keyed by its own repository, as that repository's listing keys it.
    pub items: Vec<Item>,
    /// Every one was read, not cut at [`MAX_MINE`].
    pub complete: bool,
    /// The reader's display name, which their pull requests carry.
    pub viewer: Option<String>,
    /// Each active one's review verdict, by repository and number.
    pub statuses: std::collections::HashMap<(String, u64), crate::PrStatus>,
}

/// Lists every pull request the token's owner created in `organization`, in
/// every state, a page at a time up to [`MAX_MINE`]. As with a repository's
/// pages, a full page that adds nothing new ends the listing incomplete.
pub fn mine(
    organization: &str,
    fetch: &mut impl FnMut(Call) -> crate::Result<Vec<u8>>,
) -> crate::Result<Mine> {
    let reader = reader(organization, fetch)?;
    let Some(creator) = reader.id else {
        return Err(Error::Reader.into());
    };
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut statuses = std::collections::HashMap::new();
    let mut skip = 0;
    loop {
        let page: pull_request::Page =
            parse(&fetch(Call::Get(mine_url(organization, &creator, skip)?))?)?;
        let more = page.value.len() >= PAGE;
        skip += page.value.len();
        let before = items.len();
        for node in page.value {
            let Some(source) = node.source_in(organization) else {
                continue;
            };
            if !seen.insert((source.repository.clone(), node.id())) {
                continue;
            }
            if let Some(status) = node.status() {
                statuses.insert((source.repository.clone(), node.id()), status);
            }
            let web = Repository::of(&source)?.repository_web()?;
            items.push(node.into_item(&source, &web));
        }
        let complete = !more;
        if complete || items.len() >= MAX_MINE || items.len() == before {
            items.truncate(MAX_MINE);
            return Ok(Mine {
                items,
                complete,
                viewer: reader.name,
                statuses,
            });
        }
    }
}
