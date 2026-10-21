//! Finding a checkout's repository on its host: the git common directory the
//! shared database is keyed by, the main checkout, Beads, and the remote whose
//! issues and pull requests the orchestrator lists.

use super::{Error, Provider, Repository, Result, SourceKey, error::script};
use crate::teleport::Host;
use herdr_client::{ConnectTarget, shell_quote};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// What one checkout's repository offers. The app's cache keeps the last
/// one found per checkout, to show before git answers again.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RepoInfo {
    /// The key of the shared database: the Git common directory, or `.beads`
    /// for a standalone Beads project.
    pub(crate) repository: Repository,
    /// The main checkout, where `bd` runs and Beads' source is keyed.
    pub(crate) main_root: String,
    pub(crate) beads: bool,
    /// The remote issues and pull requests come from: `origin`, else
    /// `upstream`, else the first one, as agent-launcher picks.
    pub(crate) remote: Option<Remote>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Remote {
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) source: SourceKey,
}

impl RepoInfo {
    /// The Beads source of this repository, keyed as agent-launcher keys it.
    pub(crate) fn beads_source(&self) -> Option<SourceKey> {
        self.beads.then(|| SourceKey {
            provider: Provider::Beads,
            host: "local".into(),
            repository: self.main_root.clone(),
        })
    }
}

/// Reads the repository holding `checkout` on `target`.
pub(crate) fn detect(
    target: &ConnectTarget,
    host: &Host,
    checkout: &str,
    cancelled: &AtomicBool,
) -> Result<RepoInfo> {
    if host.is_local() {
        return detect_local(target, checkout);
    }
    let output = host
        .capture(&detection_script(checkout), cancelled)
        .map_err(script("Reading the repository"))?;
    let output = String::from_utf8(output).map_err(|_| Error::Output {
        operation: "Reading the repository",
    })?;
    parse(target, &output)
}

/// The SSH probe is also exercised under a local shell in regression tests.
pub(super) fn detection_script(checkout: &str) -> String {
    format!(
        "checkout={}\n{}",
        shell_quote(checkout),
        include_str!("repo/discover.sh")
    )
}

/// The GitHub.com repository, as `owner/name`, whose issues the orchestrator
/// would list for `checkout` on `target`, or `None` for another provider.
/// Blocks on Git or SSH: call it off the UI thread.
pub(crate) fn github_repository(
    target: &ConnectTarget,
    checkout: &str,
    cancelled: &AtomicBool,
) -> Result<Option<String>> {
    let host = Host::new(target).map_err(|_| Error::UnsupportedHost)?;
    let info = detect(target, &host, checkout, cancelled)?;
    Ok(info
        .remote
        .map(|remote| remote.source)
        .filter(|source| source.provider == Provider::Github && source.host == "github.com")
        .map(|source| source.repository))
}

pub(super) fn parse(target: &ConnectTarget, output: &str) -> Result<RepoInfo> {
    if output == "not-repository\n" {
        return Err(Error::NotARepository);
    }
    let mut common = None;
    let mut main = None;
    let mut beads = false;
    let mut remotes = Vec::new();
    for line in output.lines() {
        let mut fields = line.splitn(3, '\t');
        match (fields.next(), fields.next(), fields.next()) {
            (Some("common"), Some(path), None) if !path.is_empty() => {
                common = Some(path.to_owned());
            }
            (Some("main"), Some(path), None) if !path.is_empty() => {
                main = Some(path.to_owned());
            }
            (Some("beads"), Some("1"), None) => beads = true,
            (Some("remote"), Some(name), Some(url)) => remotes.push((name, url)),
            _ => {}
        }
    }
    let (Some(common), Some(main_root)) = (common, main) else {
        return Err(Error::Output {
            operation: "Reading the repository",
        });
    };
    Ok(assemble(target, common, main_root, beads, &remotes))
}

/// What [`detect`]'s script reads, read with gix: this machine starts no
/// shell or `git`, so every client platform reads its own checkouts.
fn detect_local(target: &ConnectTarget, checkout: &str) -> Result<RepoInfo> {
    let Some(found) = super::git::checkout(Path::new(checkout))? else {
        if !Path::new(checkout).join(".beads").is_dir() {
            return Err(Error::NotARepository);
        }
        let root = Path::new(checkout)
            .canonicalize()
            .map(plain)
            .map_err(|source| Error::Git {
                operation: "Resolving the Beads folder",
                source: Box::new(source),
            })?;
        return Ok(assemble(
            target,
            root.join(".beads").to_string_lossy().into_owned(),
            root.to_string_lossy().into_owned(),
            true,
            &[],
        ));
    };
    let main = found.main.to_string_lossy().into_owned();
    let beads = found.main.join(".beads").is_dir();
    let remotes: Vec<_> = found
        .remotes
        .iter()
        .map(|(name, url)| (name.as_str(), url.as_str()))
        .collect();
    Ok(assemble(
        target,
        found.common.to_string_lossy().into_owned(),
        main,
        beads,
        &remotes,
    ))
}

/// `path` without Windows' `\\?\` prefix, which `canonicalize` adds and Git
/// and the user never write.
pub(super) fn plain(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    if let Some(rest) = path.to_str().and_then(|text| text.strip_prefix(r"\\?\"))
        && !rest.starts_with(r"UNC\")
    {
        return PathBuf::from(rest);
    }
    path
}

/// A repository's info from its git common directory, main checkout, Beads,
/// and `(name, url)` remotes.
fn assemble(
    target: &ConnectTarget,
    common: String,
    main_root: String,
    beads: bool,
    remotes: &[(&str, &str)],
) -> RepoInfo {
    let repository = match target {
        ConnectTarget::Ssh { target, .. } => Repository::Ssh {
            destination: target.clone(),
            git_dir: common,
        },
        _ => Repository::Local(common.into()),
    };
    let chosen = remotes
        .iter()
        .find(|(name, _)| *name == "origin")
        .or_else(|| remotes.iter().find(|(name, _)| *name == "upstream"))
        .or_else(|| remotes.first());
    let remote = chosen.and_then(|(name, url)| {
        Some(Remote {
            name: (*name).to_owned(),
            url: (*url).to_owned(),
            source: parse_remote_url(url).ok()?,
        })
    });
    RepoInfo {
        repository,
        main_root,
        beads,
        remote,
    }
}

/// A remote URL's host and repository path, as agent-launcher reads them, so
/// both applications key the same source.
pub(crate) fn parse_remote_url(value: &str) -> Result<SourceKey> {
    let invalid = || Error::RemoteUrl(value.to_owned());
    let (host, path) = match url::Url::parse(value) {
        Ok(url) => {
            if !matches!(url.scheme(), "http" | "https" | "ssh" | "git") {
                return Err(invalid());
            }
            let name = url.host_str().ok_or_else(invalid)?;
            let name = if name.contains(':') {
                format!("[{name}]")
            } else {
                name.to_owned()
            };
            let host = url
                .port()
                .filter(|_| matches!(url.scheme(), "http" | "https"))
                .map_or(name.clone(), |port| format!("{name}:{port}"));
            (host, url.path().to_owned())
        }
        Err(_) => {
            let (authority, path) = value.split_once(':').ok_or_else(invalid)?;
            let host = authority
                .rsplit_once('@')
                .map_or(authority, |(_, host)| host);
            if host.is_empty() || path.is_empty() || host.contains('/') {
                return Err(invalid());
            }
            (host.to_owned(), path.to_owned())
        }
    };
    let trimmed = path.trim_matches('/');
    let repository = trimmed.strip_suffix(".git").unwrap_or(trimmed).to_owned();
    if repository.split('/').count() < 2 || repository.ends_with('/') {
        return Err(invalid());
    }
    let host = host.to_ascii_lowercase();
    let provider = if host.contains("gitlab") {
        Provider::Gitlab
    } else if host.contains("github") || host.contains("ghe") {
        Provider::Github
    } else if repository.matches('/').count() > 1 {
        // Nested namespaces exist on GitLab, not GitHub.
        Provider::Gitlab
    } else {
        Provider::Github
    };
    Ok(SourceKey {
        provider,
        host,
        repository,
    })
}
