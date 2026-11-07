//! A checkout on GitLab: the project its remote names, read with the token
//! the Orchestrator reads it with, and its most recently updated open
//! issues. Merge requests are not listed. Every step runs on the worker.

use super::{Forge, Item, Kind, Origin, Target, model::LIMIT};
use crate::pull_request::{Remote, clean, local_checkout};
use sheprd_orchestrator::{
    Provider, SourceKey,
    gitlab::{self, Api, Scheme, cli_hosts},
    repo::parse_remote_url,
};
use std::time::Instant;

/// The GitLab project a remote reaches.
pub(super) struct Project {
    pub(super) source: SourceKey,
    pub(super) remote: Remote,
    /// The remote's URL, whose scheme the API is read over.
    url: String,
}

/// The GitLab project behind `target`'s `origin` remote, else its
/// `upstream`, as the Orchestrator picks; `None` for any other forge. A host
/// without "gitlab" in its name counts once glab knows it as an instance.
pub(super) fn project(
    target: &Target,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Option<Project>> {
    for remote in [Remote::Origin, Remote::Upstream] {
        let Some(url) = remote_url(target, remote, deadline, cancelled)? else {
            continue;
        };
        let Ok(mut source) = parse_remote_url(&url) else {
            return Ok(None);
        };
        gitlab::recognize(&mut source, cli_hosts::read);
        return Ok((source.provider == Provider::Gitlab).then_some(Project {
            source,
            remote,
            url,
        }));
    }
    Ok(None)
}

/// `remote`'s URL in `target`'s repository, when it has one.
fn remote_url(
    target: &Target,
    remote: Remote,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Option<String>> {
    let key = format!("remote.{}.url", remote.name());
    let timeout = deadline
        .checked_duration_since(Instant::now())
        .filter(|timeout| !timeout.is_zero())
        .ok_or(sheprd_github::pull_request::Error::Timeout)?;
    let value = match &target.host {
        crate::pull_request::Origin::Ssh(host) => {
            let directory = target
                .input
                .repo_key
                .as_deref()
                .ok_or(sheprd_github::pull_request::Error::Metadata)?;
            sheprd_client::remote_config_value(host, directory, &key, timeout, cancelled)?
        }
        crate::pull_request::Origin::Local => {
            let checkout = local_checkout(&target.input, deadline, cancelled)?;
            let mut command = sheprd_client::new_command("git");
            command
                .args(["-c", "core.fsmonitor=false", "-C", &checkout])
                .args(["config", "--get", &key]);
            let (ok, output) = crate::pull_request::run(&mut command, deadline, cancelled)?;
            ok.then(|| output.trim().to_owned())
        }
    };
    Ok(value.filter(|url| !url.is_empty()))
}

/// `project`'s most recently updated open issues, read with the token
/// `store` and the checkout's own `.envrc` give its instance.
pub(super) fn list(
    target: &Target,
    project: &Project,
    store: sheprd_credentials::Store,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
    cooldown: &mut Option<std::time::Duration>,
) -> crate::Result<(Origin, Vec<Item>)> {
    let scheme = Scheme::of_remote(&project.url);
    let api = Api::resolve(&project.source.host, scheme);
    // A checkout on this computer may set the token in its `.envrc`.
    let checkout = match target.host {
        crate::pull_request::Origin::Local => {
            Some(local_checkout(&target.input, deadline, cancelled)?)
        }
        crate::pull_request::Origin::Ssh(_) => None,
    };
    let token = gitlab::token(
        &api.instance,
        store,
        checkout.as_deref().map(std::path::Path::new),
        cancelled,
    );
    let found = gitlab::recent_issues(
        (&project.source, &api),
        token.as_ref().map(|found| &found.token),
        LIMIT,
        cooldown,
    )
    .map_err(|error| match (error, &token) {
        (sheprd_orchestrator::Error::Gitlab(error), Some(found)) => {
            sheprd_orchestrator::Error::Gitlab(error.from_source(found.source))
        }
        (error, _) => error,
    })?;
    let (owner, repo) = project
        .source
        .repository
        .rsplit_once('/')
        .unwrap_or(("", &project.source.repository));
    let origin = Origin {
        owner: owner.to_owned(),
        repo: repo.to_owned(),
        remote: project.remote,
        fork_owner: None,
        forge: Forge::GitLab {
            host: api.instance.clone(),
        },
    };
    let items = found
        .into_iter()
        .filter_map(|found| {
            item(
                &found.key.native_id,
                &found.title,
                found.author.as_deref(),
                &project.source.repository,
                project.remote,
                &api,
            )
        })
        .collect();
    Ok((origin, items))
}

/// The row for issue `native_id` of `repository`, its remote text cleaned;
/// `None` for an id that is not an issue number.
pub(super) fn item(
    native_id: &str,
    title: &str,
    author: Option<&str>,
    repository: &str,
    remote: Remote,
    api: &Api,
) -> Option<Item> {
    let number = native_id.parse::<u64>().ok().filter(|number| *number > 0)?;
    Some(Item {
        kind: Kind::Issue,
        number,
        title: clean(title),
        // Rebuilt from the project that was asked for, so a response can
        // never point a row at another project.
        url: format!(
            "{}://{}/{}/-/issues/{number}",
            api.scheme.as_str(),
            api.instance,
            repository
        ),
        author: clean(author.unwrap_or_default()),
        head: None,
        fork_owner: None,
        remote,
        draft: false,
    })
}
