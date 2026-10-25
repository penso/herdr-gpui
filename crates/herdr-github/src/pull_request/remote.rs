//! Which GitHub repository a checkout's pull requests belong to, and the Git
//! remote that reaches it. A fork checkout names the base repository with an
//! `upstream` remote and pushes to its own `origin`.

use super::fetch::{git, origin_repository};
use std::time::Instant;

/// A Git remote this client reads pull request refs from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Remote {
    #[default]
    Origin,
    Upstream,
}

impl Remote {
    pub fn name(self) -> &'static str {
        match self {
            Self::Origin => "origin",
            Self::Upstream => "upstream",
        }
    }
}

/// The repository a checkout's pull requests are opened against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base {
    pub owner: String,
    pub repo: String,
    pub remote: Remote,
    /// For an `upstream` base, the owner of the GitHub fork behind `origin`,
    /// where this checkout's own branches are pushed. `None` when `origin` is
    /// not on GitHub.com, or when the base is `origin` itself.
    pub fork_owner: Option<String>,
}

impl Base {
    /// The `headRefName` a new pull request from `branch`, pushed to
    /// `origin`, names: qualified with the fork's owner when it crosses
    /// repositories, as GitHub requires.
    pub fn head(&self, branch: &str) -> crate::Result<String> {
        match self.remote {
            Remote::Origin => Ok(branch.to_owned()),
            Remote::Upstream => {
                let fork = self.fork_owner.as_deref().ok_or(super::Error::Origin)?;
                Ok(format!("{fork}:{branch}"))
            }
        }
    }

    /// Whether a pull request from `branch` would ask to merge a branch into
    /// itself. Across a fork the same name is two different branches.
    pub fn is_own_base(&self, branch: &str, default_branch: &str) -> bool {
        self.remote == Remote::Origin && branch == default_branch
    }
}

/// A GitHub `upstream` remote when the checkout has one that names another
/// repository than origin, otherwise origin. An `upstream` that is not on
/// GitHub.com, or that names origin's own repository, is ignored rather than
/// fatal, so such checkouts keep the ordinary same-repository path.
pub fn base_repository(
    checkout: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Base> {
    let origin = origin_repository(checkout, deadline, cancelled);
    if let Ok(upstream) = git(
        checkout,
        &["config", "--default", "", "--get", "remote.upstream.url"],
        deadline,
        cancelled,
    ) && let Some((owner, repo)) = crate::github_repo(&upstream)
        && !origin
            .as_ref()
            .is_ok_and(|origin| same_repository(origin, (&owner, &repo)))
    {
        let fork_owner = origin
            .ok()
            .map(|(fork, _)| fork)
            .filter(|fork| !fork.eq_ignore_ascii_case(&owner));
        return Ok(Base {
            owner,
            repo,
            remote: Remote::Upstream,
            fork_owner,
        });
    }
    let (owner, repo) = origin?;
    Ok(Base {
        owner,
        repo,
        remote: Remote::Origin,
        fork_owner: None,
    })
}

/// GitHub owner and repository names are case-insensitive.
fn same_repository((owner, repo): &(String, String), other: (&str, &str)) -> bool {
    owner.eq_ignore_ascii_case(other.0) && repo.eq_ignore_ascii_case(other.1)
}
