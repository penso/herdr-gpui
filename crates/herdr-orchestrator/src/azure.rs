//! An Azure DevOps repository's active pull requests and its project's open
//! work items, read through the Azure DevOps REST API. Pull requests are keyed
//! `pr/<id>` as GitHub's are, and work items by their id, so both lists show
//! them beside the other providers'.
//!
//! A source's host is always `dev.azure.com` and its repository
//! `organization/project/repository`, whichever of the remote forms named it:
//! `https://dev.azure.com/…/_git/…`, `git@ssh.dev.azure.com:v3/…`, or the
//! older `https://organization.visualstudio.com/…/_git/…`.
//!
//! The credential comes from `AZURE_DEVOPS_EXT_PAT`, else from the Azure CLI's
//! sign-in (`token`). Without one, nothing is read: Azure DevOps answers an
//! anonymous request with a sign-in page.

mod error;
pub mod pull_request;
pub mod token;
pub mod work_item;

pub use error::Error;

use super::{Checkpoint, Item, Provider, SourceKey};
use chrono::Utc;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, de::DeserializeOwned};
use std::{collections::HashSet, io::Read, time::Duration};

/// Where every Azure DevOps source is keyed and read.
pub const HOST: &str = "dev.azure.com";
const API_VERSION: &str = "7.1";
const PAGE: usize = 100;
/// The most active pull requests one sync reads.
pub const MAX_OPEN: usize = 2_000;
/// Work item ids one batch read may name.
const BATCH: usize = 200;
const TIMEOUT: Duration = Duration::from_secs(30);
/// One page of 100 pull requests or 200 work items, descriptions included.
const LIMIT: u64 = 8 * 1024 * 1024;

/// A person as Azure DevOps names one.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    display_name: Option<String>,
    unique_name: Option<String>,
}

impl Identity {
    /// The display name, else the sign-in name.
    fn name(self) -> Option<String> {
        self.display_name
            .filter(|name| !name.is_empty())
            .or(self.unique_name)
    }
}

/// The source a remote on `host` at `path` names, when it is an Azure DevOps
/// repository. `path` may still carry percent escapes, as a URL's path does.
pub fn source(host: &str, path: &str) -> Option<SourceKey> {
    let host = host.to_ascii_lowercase();
    let decoded = percent_decode_str(path.trim_matches('/'))
        .decode_utf8()
        .ok()?;
    let segments: Vec<&str> = decoded.split('/').filter(|part| !part.is_empty()).collect();
    let (organization, project, repository) = match (host.as_str(), &segments[..]) {
        // `https://dev.azure.com/org/project/_git/repo`
        (HOST, [organization, project, "_git", repository]) => {
            (*organization, *project, *repository)
        }
        // `git@ssh.dev.azure.com:v3/org/project/repo`, and the older SSH host
        (
            "ssh.dev.azure.com" | "vs-ssh.visualstudio.com",
            ["v3", organization, project, repository],
        ) => (*organization, *project, *repository),
        // `https://org.visualstudio.com[/DefaultCollection]/project/_git/repo`
        (host, rest) if host.ends_with(".visualstudio.com") => {
            let organization = host.strip_suffix(".visualstudio.com")?;
            match rest {
                [project, "_git", repository] | [_, project, "_git", repository] => {
                    (organization, *project, *repository)
                }
                _ => return None,
            }
        }
        _ => return None,
    };
    let repository = repository.strip_suffix(".git").unwrap_or(repository);
    [organization, project, repository]
        .iter()
        .all(|part| !part.is_empty())
        .then(|| SourceKey {
            provider: Provider::Azure,
            host: HOST.to_owned(),
            repository: format!("{organization}/{project}/{repository}"),
        })
}

/// A source's organization, project, and repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Repository<'a> {
    pub organization: &'a str,
    pub project: &'a str,
    pub name: &'a str,
}

impl<'a> Repository<'a> {
    pub fn of(source: &'a SourceKey) -> Result<Self, Error> {
        let mut parts = source.repository.splitn(3, '/');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(organization), Some(project), Some(name))
                if !organization.is_empty() && !project.is_empty() && !name.is_empty() =>
            {
                Ok(Self {
                    organization,
                    project,
                    name,
                })
            }
            _ => Err(Error::Address(source.repository.clone())),
        }
    }

    /// `https://dev.azure.com/org/project/` followed by `segments`, each
    /// escaped as one path segment.
    fn url(&self, segments: &[&str]) -> Result<url::Url, Error> {
        let invalid = || Error::Address(format!("{}/{}", self.organization, self.project));
        let mut url = url::Url::parse(&format!("https://{HOST}")).map_err(|_| invalid())?;
        url.path_segments_mut()
            .map_err(|()| invalid())?
            .push(self.organization)
            .push(self.project)
            .extend(segments);
        Ok(url)
    }

    /// The project's web address, which work item links hang from.
    pub fn web(&self) -> Result<String, Error> {
        Ok(self.url(&[])?.as_str().trim_end_matches('/').to_owned())
    }

    /// The repository's web address, which pull request links hang from.
    pub fn repository_web(&self) -> Result<String, Error> {
        Ok(self.url(&["_git", self.name])?.as_str().to_owned())
    }

    /// The repository's active pull requests, `skip` of them passed.
    pub fn pull_requests_url(&self, skip: usize) -> Result<url::Url, Error> {
        let mut url = self.url(&["_apis", "git", "repositories", self.name, "pullrequests"])?;
        url.query_pairs_mut()
            .append_pair("searchCriteria.status", "active")
            .append_pair("$top", &PAGE.to_string())
            .append_pair("$skip", &skip.to_string())
            .append_pair("api-version", API_VERSION);
        Ok(url)
    }

    /// The project's WIQL endpoint, capped at [`work_item::MAX_OPEN`].
    pub fn query_url(&self) -> Result<url::Url, Error> {
        let mut url = self.url(&["_apis", "wit", "wiql"])?;
        url.query_pairs_mut()
            .append_pair("$top", &work_item::MAX_OPEN.to_string())
            .append_pair("api-version", API_VERSION);
        Ok(url)
    }

    /// The fields of work items `ids`.
    pub fn work_items_url(&self, ids: &[u64]) -> Result<url::Url, Error> {
        let ids = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        let mut url = self.url(&["_apis", "wit", "workitems"])?;
        url.query_pairs_mut()
            .append_pair("ids", &ids)
            .append_pair("fields", work_item::FIELDS)
            .append_pair("errorPolicy", "omit")
            .append_pair("api-version", API_VERSION);
        Ok(url)
    }
}

/// One sync's result. Only a `complete` listing may replace the cache.
pub struct Listing {
    pub items: Vec<Item>,
    pub checkpoint: Checkpoint,
    pub complete: bool,
}

/// A request Azure DevOps answers with JSON.
pub enum Call {
    Get(url::Url),
    Post(url::Url, serde_json::Value),
}

/// Lists `source`'s active pull requests and its project's open work items
/// through `fetch`, which sends one request. `previous` keeps agent-launcher's
/// markers, which herdr-gpui never adds.
pub fn list(
    source: &SourceKey,
    previous: Option<&Checkpoint>,
    fetch: &mut impl FnMut(Call) -> super::Result<Vec<u8>>,
) -> super::Result<Listing> {
    let started = Utc::now();
    let repository = Repository::of(source)?;
    let (mut items, mut complete) = work_items(source, &repository, fetch)?;
    let (pull_requests, listed) = pull_requests(source, &repository, fetch)?;
    items.extend(pull_requests);
    complete &= listed;
    let previous = previous.cloned().unwrap_or_default();
    Ok(Listing {
        items,
        checkpoint: Checkpoint {
            updated_at: Some(started),
            etag: None,
            last_full_at: Some(Utc::now()),
            pr_details: previous.pr_details,
            pr_cursor: previous.pr_cursor,
        },
        complete,
    })
}

/// Every active pull request, a page at a time, and whether all were read.
/// Pages are numbered, so one closed while they are read may shift another
/// out of view for this sync; the next sync lists it again.
fn pull_requests(
    source: &SourceKey,
    repository: &Repository<'_>,
    fetch: &mut impl FnMut(Call) -> super::Result<Vec<u8>>,
) -> super::Result<(Vec<Item>, bool)> {
    let web = repository.repository_web()?;
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    loop {
        let page: pull_request::Page = parse(&fetch(Call::Get(
            repository.pull_requests_url(items.len())?,
        ))?)?;
        let more = page.value.len() >= PAGE;
        items.extend(
            page.value
                .into_iter()
                .filter(|node| seen.insert(node.id()))
                .map(|node| node.into_item(source, &web)),
        );
        if !more {
            return Ok((items, true));
        }
        if items.len() >= MAX_OPEN {
            return Ok((items, false));
        }
    }
}

/// The project's open work items, most recently changed first, and whether
/// the query named all of them.
fn work_items(
    source: &SourceKey,
    repository: &Repository<'_>,
    fetch: &mut impl FnMut(Call) -> super::Result<Vec<u8>>,
) -> super::Result<(Vec<Item>, bool)> {
    let query = serde_json::json!({ "query": work_item::QUERY });
    let reply: work_item::QueryReply = parse(&fetch(Call::Post(repository.query_url()?, query))?)?;
    let complete = reply.work_items.len() < work_item::MAX_OPEN;
    let ids: Vec<u64> = reply.work_items.iter().map(|item| item.id).collect();
    let web = repository.web()?;
    let mut items = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(BATCH) {
        let batch: work_item::Batch = parse(&fetch(Call::Get(repository.work_items_url(chunk)?))?)?;
        // The batch comes back in id order; keep the query's.
        let mut nodes: Vec<_> = batch.value;
        nodes.sort_by_key(|node| {
            chunk
                .iter()
                .position(|id| *id == node.id())
                .unwrap_or(usize::MAX)
        });
        items.extend(nodes.into_iter().map(|node| node.into_item(source, &web)));
    }
    Ok((items, complete))
}

fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    serde_json::from_slice(bytes).map_err(Error::Json)
}

/// Lists `source` with `token` over HTTPS. Blocks on the network, which
/// `cancelled` stops between requests: call it from a worker.
pub fn sync(
    source: &SourceKey,
    token: Option<&token::Token>,
    previous: Option<&Checkpoint>,
    cancelled: &impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> super::Result<Listing> {
    let Some(token) = token else {
        return Err(Error::SignIn {
            repository: source.repository.clone(),
        }
        .into());
    };
    let agent = agent(TIMEOUT);
    list(source, previous, &mut |call| {
        if cancelled() {
            return Err(super::Error::Cancelled);
        }
        Ok(send(&agent, source, token, call, cooldown)?)
    })
}

fn send(
    agent: &ureq::Agent,
    source: &SourceKey,
    token: &token::Token,
    call: Call,
    cooldown: &mut Option<Duration>,
) -> Result<Vec<u8>, Error> {
    let authorization = token.authorization()?;
    let network = |error| Error::Network {
        origin: format!("https://{HOST}"),
        source: Box::new(error),
    };
    let mut reply = match call {
        Call::Get(url) => agent
            .get(url.as_str())
            .header("User-Agent", "Herdr-GPUI")
            .header("Accept", "application/json")
            .header("Authorization", authorization)
            .call()
            .map_err(network)?,
        Call::Post(url, body) => agent
            .post(url.as_str())
            .header("User-Agent", "Herdr-GPUI")
            .header("Accept", "application/json")
            .header("Authorization", authorization)
            .header("Content-Type", "application/json")
            .send(&serde_json::to_vec(&body).map_err(Error::Json)?[..])
            .map_err(network)?,
    };
    let status = reply.status().as_u16();
    if status != 200 {
        *cooldown = retry_after(status, reply.headers());
        tracing::warn!(
            category = "orchestrator_azure",
            status,
            "Azure DevOps refused a listing"
        );
        return Err(status_error(status, &source.repository));
    }
    body(&mut reply, LIMIT)
}

/// An agent for Azure DevOps's API: bounded by `timeout`, following no
/// redirect, so a token is never carried to a sign-in page or elsewhere, and
/// returning refusals as statuses to read.
fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

fn body(reply: &mut ureq::http::Response<ureq::Body>, limit: u64) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    reply
        .body_mut()
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::Read)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Size);
    }
    Ok(bytes)
}

/// What a refusal means. Azure DevOps answers a token it does not accept with
/// 401, or with a 203 sign-in page, and a redirect to sign in otherwise.
pub fn status_error(status: u16, repository: &str) -> Error {
    let repository = repository.to_owned();
    match status {
        203 | 302 | 401 => Error::Authentication,
        403 => Error::Forbidden { repository },
        404 => Error::NotFound { repository },
        429 => Error::RateLimit,
        status => Error::Status(status),
    }
}

/// How long a rate-limited source waits: `retry-after` seconds, bounded to
/// between five minutes and a day.
pub fn retry_after(status: u16, headers: &ureq::http::HeaderMap) -> Option<Duration> {
    if status != 429 {
        return None;
    }
    let wait = headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(3600);
    Some(Duration::from_secs(wait.clamp(300, 86400)))
}
