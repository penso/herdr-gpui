//! The two approved repository identities. A transfer changes location, never
//! the version, asset, channel, or trust anchor of a request.
use super::{Error, Result, allowed_url, parse_version};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Repository {
    Herdr,
    Sheprd,
}

impl Repository {
    pub(super) const ALL: &[Self] = &[Self::Herdr, Self::Sheprd];

    fn path(self) -> &'static str {
        match self {
            Self::Herdr => "penso/herdr-gpui",
            Self::Sheprd => "sheprd-labs/sheprd",
        }
    }

    pub(super) fn asset(self, version: &str, name: &str) -> String {
        format!(
            "https://github.com/{}/releases/download/v{version}/{name}",
            self.path()
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Resource<'a> {
    Metadata(&'a str),
    Asset { version: &'a str, name: &'a str },
}

impl Resource<'_> {
    fn url(self, repository: Repository) -> String {
        match self {
            Self::Metadata(suffix) => {
                format!(
                    "https://api.github.com/repos/{}/releases{suffix}",
                    repository.path()
                )
            }
            Self::Asset { version, name } => repository.asset(version, name),
        }
    }
}

fn resource(url: &str) -> Option<(Repository, Resource<'_>)> {
    if !allowed_url(url) {
        return None;
    }
    for &repository in Repository::ALL {
        let api = format!(
            "https://api.github.com/repos/{}/releases",
            repository.path()
        );
        if let Some(suffix) = url.strip_prefix(&api)
            && matches!(suffix, "/latest" | "?per_page=10" | "?per_page=20")
        {
            return Some((repository, Resource::Metadata(suffix)));
        }
        let download = format!(
            "https://github.com/{}/releases/download/v",
            repository.path()
        );
        if let Some((version, name)) = url
            .strip_prefix(&download)
            .and_then(|rest| rest.split_once('/'))
            && parse_version(version).is_some()
            && !name.is_empty()
            && name.len() <= 255
            && !matches!(name, "." | "..")
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        {
            return Some((repository, Resource::Asset { version, name }));
        }
    }
    None
}

pub(super) fn is_metadata(url: &str) -> bool {
    matches!(resource(url), Some((_, Resource::Metadata(_))))
}

/// No parallel feeds or error-driven downgrade: only absence at the old
/// address permits the same resource at the new address. Never retry a body,
/// signature, authorization, rate-limit, or network failure as another feed.
pub(super) fn fetch<T>(url: &str, mut fetch: impl FnMut(&str) -> Result<T>) -> Result<T> {
    let (repository, resource) = resource(url).ok_or(Error::UntrustedUrl)?;
    match fetch(url) {
        Err(Error::HttpStatus(404)) if repository == Repository::Herdr => {
            fetch(&resource.url(Repository::Sheprd))
        }
        result => result,
    }
}

/// Validate against the ORIGINAL resource at every hop, not just the host of
/// the previous redirect. Signed CDN queries are opaque, and only asset
/// requests may reach the release CDN.
pub(super) fn redirect(original: &str, next: &str) -> Result<String> {
    let (repository, expected) = resource(original).ok_or(Error::UntrustedUrl)?;
    if !allowed_url(next) {
        return Err(Error::UntrustedUrl);
    }
    if resource(next).is_some_and(|(_, actual)| actual == expected) {
        return Ok(next.to_owned());
    }
    match expected {
        Resource::Asset { .. }
            if next.starts_with("https://release-assets.githubusercontent.com/") =>
        {
            Ok(next.to_owned())
        }
        Resource::Metadata(suffix) if repository == Repository::Herdr => {
            // GitHub's moved-repository API can return /repositories/NUMERIC_ID
            // instead of the canonical slug. Never trust/fetch an arbitrary ID:
            // re-address the same query to the explicitly approved successor.
            let id = next
                .strip_prefix("https://api.github.com/repositories/")
                .and_then(|rest| rest.strip_suffix(&format!("/releases{suffix}")));
            if id.is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())) {
                Ok(expected.url(Repository::Sheprd))
            } else {
                Err(Error::UntrustedUrl)
            }
        }
        _ => Err(Error::UntrustedUrl),
    }
}

pub(super) fn initial(url: &str) -> Result<()> {
    resource(url).map(|_| ()).ok_or(Error::UntrustedUrl)
}
