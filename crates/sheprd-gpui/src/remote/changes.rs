//! Answers phones' questions about a workspace (`control::changes`) with
//! what Review reads, shaped for a small screen. Windows say which workspaces
//! exist ([`Sources`]), so a phone reaches only those, and only listed files
//! and failing checks within them. Nothing here writes to a repository, and
//! everything but [`Source::new`] blocks, so it runs on a background thread.

use crate::{
    pull_request::{CheckDetail, Input, Origin, Outcome, PullRequest, workspace_input},
    review::diff::{self, Body, Kind, Request, Scope as ReviewScope, Status},
};
use secrecy::SecretString;
use sheprd_client::protocol::ClientShellWorkspace;
use sheprd_github::{Tokens, check_log, pr_actions::Target};
use sheprd_remote_control::control::changes::{
    Answer, Ask, ChangedFile, Check, CheckLog, Checks, DiffLine, Failure, FileDiff, FileList,
    FileStatus, LineKind, MAX_CHECKS, Scope, Shown, Wanted,
};
use std::{collections::HashMap, sync::Arc};

/// The workspaces of one daemon session a phone may ask about.
#[derive(Default)]
pub(crate) struct Sources {
    pub(super) boot_id: String,
    pub(super) workspaces: HashMap<String, Source>,
}

impl Sources {
    /// Works out the answer to `ask`. A workspace of another session is one
    /// this client cannot speak for. Blocking.
    pub(super) fn answer(&self, ask: &Ask) -> Answer {
        let source = (ask.boot_id == self.boot_id)
            .then(|| self.workspaces.get(&ask.workspace_id))
            .flatten();
        let Some(source) = source else {
            return Answer::Failed(Failure::Gone);
        };
        source.answer(&ask.wanted).unwrap_or_else(Answer::Failed)
    }
}

/// A workspace, and its pull request as the desktop's cache holds it.
pub(crate) struct Source {
    pull_request: Option<Checked>,
    workspace: ClientShellWorkspace,
}

/// The parts of a pull request a phone reads.
struct Checked {
    base: String,
    checks: Vec<Check>,
    logs: Option<Logs>,
    number: u64,
}

/// What reading a job's log takes.
struct Logs {
    owner: String,
    repo: String,
    token: Arc<SecretString>,
}

impl Source {
    pub(crate) fn new(
        workspace: ClientShellWorkspace,
        pull_request: Option<&PullRequest>,
        tokens: Option<&Tokens>,
    ) -> Self {
        Self {
            pull_request: pull_request.map(|pr| Checked::new(pr, tokens)),
            workspace,
        }
    }

    fn answer(&self, wanted: &Wanted) -> Result<Answer, Failure> {
        match wanted {
            Wanted::Files { scope } => self.files(*scope).map(Answer::Files),
            Wanted::Diff { scope, path } => self.diff(*scope, path).map(Answer::Diff),
            Wanted::Checks => Ok(Answer::Checks(self.checks())),
            Wanted::CheckLog { job } => self.check_log(*job).map(Answer::CheckLog),
        }
    }

    fn input(&self) -> Result<Input, Failure> {
        workspace_input(&self.workspace, &Origin::Local).map_err(|error| {
            tracing::debug!(%error, "A phone asked about a workspace without a repository");
            Failure::NoRepository
        })
    }

    fn base(&self) -> Option<&str> {
        self.pull_request.as_ref().map(|pr| pr.base.as_str())
    }

    fn files(&self, scope: Scope) -> Result<FileList, Failure> {
        files(&self.input()?, self.base(), scope)
    }

    fn diff(&self, scope: Scope, path: &str) -> Result<FileDiff, Failure> {
        file_diff(&self.input()?, self.base(), scope, path)
    }

    fn checks(&self) -> Checks {
        self.pull_request
            .as_ref()
            .map_or_else(Checks::default, |pr| Checks {
                number: Some(pr.number),
                checks: pr.checks.clone(),
            })
    }

    fn check_log(&self, job: u64) -> Result<CheckLog, Failure> {
        let pr = self.pull_request.as_ref().ok_or(Failure::NoLog)?;
        let logs = pr.logs.as_ref().ok_or(Failure::NoLog)?;
        // Only a job this pull request lists as failing: a phone does not
        // get to read any log the account can.
        if !pr.checks.iter().any(|check| check.job == Some(job)) {
            return Err(Failure::NoLog);
        }
        let log =
            check_log::job_log(&logs.token, &logs.owner, &logs.repo, job).map_err(log_failure)?;
        let excerpt = check_log::excerpt(&log);
        if excerpt.is_empty() {
            return Err(Failure::NoLog);
        }
        Ok(CheckLog { excerpt })
    }
}

fn log_failure(error: sheprd_github::Error) -> Failure {
    tracing::warn!(%error, "Cannot read a failing check's log for a phone");
    match error {
        // GitHub keeps logs for a while, and only Actions has them.
        sheprd_github::Error::Status(404 | 410) => Failure::NoLog,
        _ => Failure::Unreadable,
    }
}

impl Checked {
    fn new(pr: &PullRequest, tokens: Option<&Tokens>) -> Self {
        let logs = Target::for_reading(pr)
            .ok()
            .zip(tokens)
            .map(|(target, tokens)| Logs {
                token: tokens.for_owner(&target.owner).clone(),
                owner: target.owner,
                repo: target.repo,
            });
        Self {
            base: pr.base_ref_name.clone(),
            checks: pr.check_details().take(MAX_CHECKS).map(check).collect(),
            logs,
            number: pr.number,
        }
    }
}

/// A check as a phone lists it. Only a failure offers its log.
fn check(detail: CheckDetail<'_>) -> Check {
    Check {
        name: detail.name.to_owned(),
        state: detail.outcome.into(),
        job: detail.job.filter(|_| detail.outcome == Outcome::Failed),
        url: detail.url.map(str::to_owned),
    }
}

fn load(input: &Input, base: Option<&str>, scope: Scope) -> Result<diff::Loaded, Failure> {
    let scope = match scope {
        Scope::Uncommitted => ReviewScope::Uncommitted,
        Scope::Branch => ReviewScope::Branch,
    };
    diff::load(input, scope, None, base, false).map_err(|error| match error {
        crate::Error::Review(crate::review::Error::NoBase) => Failure::NoBase,
        error => {
            tracing::warn!(%error, "Cannot list a workspace's changes for a phone");
            Failure::NoRepository
        }
    })
}

fn files(input: &Input, base: Option<&str>, scope: Scope) -> Result<FileList, Failure> {
    let loaded = load(input, base, scope)?;
    let files = loaded.diff.files.iter().map(changed_file);
    Ok(FileList::bounded(loaded.base.clone(), files))
}

fn changed_file(file: &diff::FileDiff) -> ChangedFile {
    ChangedFile {
        path: file.path.clone(),
        old_path: file.old_path.clone(),
        status: match file.status {
            Status::Added => FileStatus::Added,
            Status::Deleted => FileStatus::Deleted,
            Status::Modified => FileStatus::Modified,
            Status::Renamed => FileStatus::Renamed,
            Status::Untracked => FileStatus::Untracked,
        },
        added: file.added,
        removed: file.removed,
        shown: shown(&file.body).unwrap_or_default(),
    }
}

/// What stands in for a body with no lines to show.
fn shown(body: &Body) -> Option<Shown> {
    match body {
        Body::Binary => Some(Shown::Binary),
        Body::Large => Some(Shown::TooLarge),
        Body::Failed(_) => Some(Shown::Unreadable),
        Body::Pending | Body::Loaded(_) => None,
    }
}

fn file_diff(
    input: &Input,
    base: Option<&str>,
    scope: Scope,
    path: &str,
) -> Result<FileDiff, Failure> {
    let loaded = load(input, base, scope)?;
    let (index, file) = loaded
        .diff
        .files
        .iter()
        .enumerate()
        .find(|(_, file)| file.path == path)
        .ok_or(Failure::NoFile)?;
    if let Some(note) = shown(&file.body) {
        return Ok(FileDiff::note(path, note));
    }
    let request = Request::of(index, file);
    let body = diff::bodies(&loaded.source, &[request], false)
        .into_iter()
        .next()
        .map_or(Body::Pending, |(_, body)| body);
    match body {
        Body::Loaded(lines) => {
            let shaped = lines.iter().map(|line| DiffLine {
                kind: line_kind(line.kind),
                old: line.old,
                new: line.new,
                text: lines.text_of(line).to_owned(),
            });
            let mut diff = FileDiff::bounded(path, shaped);
            diff.truncated |= lines.truncated;
            Ok(diff)
        }
        body => Ok(FileDiff::note(
            path,
            shown(&body).unwrap_or(Shown::Unreadable),
        )),
    }
}

fn line_kind(kind: Kind) -> LineKind {
    match kind {
        Kind::Hunk => LineKind::Hunk,
        Kind::Context => LineKind::Context,
        Kind::Added => LineKind::Added,
        Kind::Removed => LineKind::Removed,
        Kind::Meta => LineKind::Meta,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
