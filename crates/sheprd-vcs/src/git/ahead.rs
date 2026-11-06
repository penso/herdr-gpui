//! Commits a branch holds beyond its repository's default branch, so the
//! chrome can say a pull request is still to be opened. A clean tree says
//! nothing about this: `Status` only compares the working tree with HEAD.
//!
//! Each listed checkout costs a few read-only processes, on the probes'
//! schedule and through the same worker. Only remote-tracking refs are read;
//! nothing here contacts a remote.

use super::{CACHE_LIMIT, git};
use sheprd_github::pull_request::{Input, Remote, base_repository, local_checkout};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// The most commits counted, which also bounds what Git prints. A branch this
/// far ahead reads the same to a person at any larger number.
const COUNT_LIMIT: &str = "--max-count=1000";

/// A branch's commits that its default branch does not have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ahead {
    pub commits: u64,
    /// The branch's tip, to tell a merged pull request's commits from newer
    /// ones: a squash merge leaves a branch ahead of its base for good.
    pub head: String,
}

/// One checkout's latest answer. A failure, like a branch with nothing ahead,
/// holds `None`, so a broken repository shows no mark.
struct Entry {
    input: Input,
    ahead: Option<Ahead>,
    due: Instant,
}

/// Every tracked checkout's answer and the ones waiting to be asked.
#[derive(Default)]
pub(super) struct Cache {
    entries: Vec<Entry>,
    queue: VecDeque<Input>,
}

impl Cache {
    /// Forget checkouts no longer listed and queue the ones that are due.
    pub(super) fn schedule(&mut self, listed: &[Input], refresh: bool, now: Instant) {
        self.entries.retain(|entry| listed.contains(&entry.input));
        self.queue.retain(|input| listed.contains(input));
        if !refresh {
            return;
        }
        for input in listed.iter().take(CACHE_LIMIT) {
            let due = self
                .entries
                .iter()
                .find(|entry| entry.input == *input)
                .is_none_or(|entry| now >= entry.due);
            if due && !self.queue.contains(input) {
                self.queue.push_back(input.clone());
            }
        }
    }

    pub(super) fn next(&mut self) -> Option<Input> {
        self.queue.pop_front()
    }

    pub(super) fn record(&mut self, input: Input, ahead: Option<Ahead>, due: Instant) {
        match self.entries.iter_mut().find(|entry| entry.input == input) {
            Some(entry) => {
                entry.ahead = ahead;
                entry.due = due;
            }
            None => self.entries.push(Entry { input, ahead, due }),
        }
    }

    /// Ask again at the next scan: a commit or push moved what it read.
    pub(super) fn expire(&mut self, input: &Input, now: Instant) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.input == *input) {
            entry.due = now;
        }
    }

    pub(super) fn get(&self, input: &Input) -> Option<&Ahead> {
        self.entries
            .iter()
            .find(|entry| entry.input == *input)
            .and_then(|entry| entry.ahead.as_ref())
    }

    pub(super) fn listed(&self, repo_key: &str, branch: &str) -> Option<&Ahead> {
        self.entries
            .iter()
            .find(|entry| {
                entry.input.repo_key.as_deref() == Some(repo_key) && entry.input.branch == branch
            })
            .and_then(|entry| entry.ahead.as_ref())
    }
}

/// What `input`'s branch holds beyond the default branch of the repository
/// its pull requests target, the one the lookup asks. `None` when it is that
/// branch, holds nothing more, or that remote names no default branch, as a
/// remote added by hand may not. Another remote's default branch would answer
/// for the wrong repository: a fork's can already hold the commits.
pub(super) fn read(
    input: &Input,
    timeout: Duration,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Option<Ahead>> {
    let deadline = Instant::now() + timeout;
    let checkout = local_checkout(input, deadline, cancelled)?;
    let remote = base_repository(&checkout, deadline, cancelled)?.remote;
    let Some((base, default)) = default_branch(&checkout, remote, deadline, cancelled)? else {
        return Ok(None);
    };
    if input.branch == default {
        return Ok(None);
    }
    let list = git(
        &checkout,
        &["rev-list", COUNT_LIMIT, &format!("{base}..HEAD")],
        "list commits ahead of the default branch",
        deadline,
        cancelled,
    )?;
    Ok(parse(&list))
}

/// `remote`'s default branch, as its full ref and its short name.
fn default_branch(
    checkout: &str,
    remote: Remote,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Option<(String, String)>> {
    let remote = remote.name();
    let head = format!("refs/remotes/{remote}/HEAD");
    let base = match git(
        checkout,
        &["symbolic-ref", "--quiet", &head],
        "read the default branch",
        deadline,
        cancelled,
    ) {
        Ok(base) => base,
        // Exits nonzero for a remote that names no default branch.
        Err(crate::Error::Git(super::Error::Failed { .. })) => return Ok(None),
        Err(error) => return Err(error),
    };
    let prefix = format!("refs/remotes/{remote}/");
    let Some(default) = base.strip_prefix(&prefix).map(str::to_owned) else {
        return Ok(None);
    };
    Ok(Some((base, default)))
}

/// `rev-list` of one start point prints that tip first, one commit per line.
fn parse(list: &str) -> Option<Ahead> {
    let head = list.lines().next().filter(|head| !head.is_empty())?;
    Some(Ahead {
        commits: list.lines().count() as u64,
        head: head.to_owned(),
    })
}
