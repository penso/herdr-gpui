//! Working-tree status and Git write operations for local checkouts.
//!
//! One worker thread owns every child process, so the UI thread only queues a
//! request and reads the result on a later tick. The focused checkout gets a
//! counted status; every other listed workspace gets a cheaper dirty probe,
//! round-robin and rate limited, so the sidebar can mark uncommitted work
//! without a process per row per frame. Commit, push and pull request creation
//! are explicit user actions and are never retried or replayed automatically.
use crate::{
    Error,
    pull_request::{Input, clean, local_checkout, origin_repository, run},
};
use secrecy::SecretString;
use std::{
    collections::VecDeque,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

/// Status refreshes touch the disk, so they are slow-polled and only for the
/// checkout the chrome is actually showing.
const REFRESH: Duration = Duration::from_secs(5);
const ERROR_BACKOFF: Duration = Duration::from_secs(60);
/// Rows the user is not working in change rarely and cost a process each, so
/// they refresh slowly and a failure waits longer still.
const PROBE_REFRESH: Duration = Duration::from_secs(30);
const PROBE_BACKOFF: Duration = Duration::from_secs(300);
/// One scan per second at most, and a bounded cache: a daemon listing hundreds
/// of workspaces cannot turn into hundreds of queued probes.
const SCAN_INTERVAL: Duration = Duration::from_secs(1);
const CACHE_LIMIT: usize = 128;
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);
/// Signing a commit or authenticating a push can wait on a hardware key.
const OPERATION_TIMEOUT: Duration = Duration::from_secs(180);
const API_TIMEOUT: Duration = Duration::from_secs(30);
const MESSAGE_LIMIT: usize = 4096;
const TITLE_LIMIT: usize = 256;

const REPOSITORY_QUERY: &str = r#"query($owner: String!, $repo: String!) {
  repository(owner: $owner, name: $repo) { id defaultBranchRef { name } }
}"#;
const CREATE_MUTATION: &str = r#"mutation($repository: ID!, $base: String!, $head: String!, $title: String!, $body: String!) {
  createPullRequest(input: {repositoryId: $repository, baseRefName: $base, headRefName: $head, title: $title, body: $body}) {
    pullRequest { number url }
  }
}"#;

/// Tracked line changes against HEAD plus the untracked entries `git add -A`
/// would also stage, so the chrome never understates what a commit includes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Status {
    pub additions: u64,
    pub deletions: u64,
    pub untracked: u64,
}

impl Status {
    pub fn dirty(&self) -> bool {
        self.additions > 0 || self.deletions > 0 || self.untracked > 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Commit(String),
    Push,
    CreatePullRequest,
}

impl Action {
    pub fn running_label(&self) -> &'static str {
        match self {
            Self::Commit(_) => "Committing...",
            Self::Push => "Pushing...",
            Self::CreatePullRequest => "Creating pull request...",
        }
    }
}

/// What an action achieved, in the user's terms. `url` is offered as a link
/// rather than opened by the worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Outcome {
    pub message: String,
    pub url: Option<String>,
}

enum Job {
    Status,
    /// Is there anything to commit? Cheaper than counting lines, and all a
    /// sidebar row needs to show its dot.
    Dirty,
    Run(Action, Option<Arc<SecretString>>),
}

enum Completion {
    Status(crate::Result<Status>),
    Dirty(crate::Result<bool>),
    Action(crate::Result<Outcome>),
}

/// One listed checkout's probe. A failure is remembered as "unknown" rather
/// than "clean", so a broken repository shows no dot instead of a wrong one.
struct Probe {
    input: Input,
    dirty: Option<bool>,
    due: Instant,
    used: Instant,
}

struct Worker {
    requests: mpsc::SyncSender<(u64, Input, Job)>,
    results: mpsc::Receiver<(Input, Completion)>,
}

/// Client-local Git state for the focused checkout. Dropping it retires every
/// in-flight request; no result of an old checkout can reach a new one.
#[derive(Default)]
pub(super) struct Git {
    worker: Option<Worker>,
    generation: Arc<AtomicU64>,
    busy: bool,
    waiting: Option<(Input, Job)>,
    input: Option<Input>,
    status: Option<Status>,
    due: Option<Instant>,
    running: Option<Action>,
    error: Option<String>,
    outcome: Option<Outcome>,
    probes: Vec<Probe>,
    queue: VecDeque<Input>,
    next_scan: Option<Instant>,
}

impl Drop for Git {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl Git {
    /// Chrome fixture: a tracked checkout and its status, with no worker and
    /// therefore no scheduled Git work.
    #[cfg(test)]
    pub fn fixture(input: Input, status: Status) -> Self {
        let mut git = Self::default();
        git.input = Some(input);
        git.status = Some(status);
        git
    }

    /// Chrome fixture: a probe answer for a listed checkout, with no worker
    /// and therefore no scheduled Git work.
    #[cfg(test)]
    pub fn seed_probe(&mut self, input: Input, dirty: bool, now: Instant) {
        self.record_probe(input, Some(dirty), now);
    }

    pub fn tracked(&self) -> Option<&Input> {
        self.input.as_ref()
    }

    pub fn status(&self) -> Option<Status> {
        self.status
    }

    pub fn running(&self) -> Option<&Action> {
        self.running.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    /// Follow the focused checkout and schedule the next read-only refresh.
    /// `refresh` is false while the window is inactive: the last known status
    /// stays on screen instead of polling Git behind the user's back.
    pub fn track(&mut self, input: Option<Input>, refresh: bool, now: Instant) -> bool {
        let mut changed = false;
        if self.input != input {
            // A running action keeps running: its result still belongs to the
            // checkout it was started for and is reported against that input.
            self.input = input;
            self.status = None;
            self.error = None;
            self.outcome = None;
            self.due = Some(now);
            changed = true;
        }
        let Some(input) = self.input.clone() else {
            return changed;
        };
        if refresh && self.waiting.is_none() && !self.busy && self.due.is_some_and(|due| now >= due)
        {
            self.due = None;
            self.waiting = Some((input, Job::Status));
        }
        changed
    }

    /// Does this checkout have anything to commit? `None` while unknown, so a
    /// row shows nothing rather than claiming a clean tree. Pure cache read:
    /// rendering never schedules Git work.
    pub fn dirty(&self, repo_key: &str, branch: &str) -> Option<bool> {
        if self
            .input
            .as_ref()
            .is_some_and(|input| input.repo_key == repo_key && input.branch == branch)
            && let Some(status) = self.status
        {
            return Some(status.dirty());
        }
        self.probes
            .iter()
            .find(|probe| probe.input.repo_key == repo_key && probe.input.branch == branch)
            .and_then(|probe| probe.dirty)
    }

    /// Follow the listed checkouts: drop probes for workspaces that are gone
    /// and queue the ones whose answer is missing or stale.
    pub fn track_listed(
        &mut self,
        inputs: impl IntoIterator<Item = Input>,
        refresh: bool,
        now: Instant,
    ) {
        let mut listed = Vec::new();
        for input in inputs.into_iter().take(CACHE_LIMIT) {
            if !listed.contains(&input) {
                listed.push(input);
            }
        }
        self.probes.retain(|probe| listed.contains(&probe.input));
        self.queue.retain(|input| listed.contains(input));
        if !refresh || self.next_scan.is_some_and(|next| now < next) {
            return;
        }
        self.next_scan = Some(now + SCAN_INTERVAL);
        for input in listed {
            // The focused checkout is counted by its own status refresh.
            if self.input.as_ref() == Some(&input) || self.queue.contains(&input) {
                continue;
            }
            if self
                .probes
                .iter()
                .find(|probe| probe.input == input)
                .is_none_or(|probe| now >= probe.due)
            {
                self.queue.push_back(input);
            }
        }
    }

    fn record_probe(&mut self, input: Input, dirty: Option<bool>, now: Instant) {
        let due = now
            + if dirty.is_some() {
                PROBE_REFRESH
            } else {
                PROBE_BACKOFF
            };
        if let Some(probe) = self.probes.iter_mut().find(|probe| probe.input == input) {
            probe.dirty = dirty;
            probe.due = due;
            probe.used = now;
            return;
        }
        if self.probes.len() == CACHE_LIMIT
            && let Some((index, _)) = self
                .probes
                .iter()
                .enumerate()
                .min_by_key(|(_, probe)| probe.used)
        {
            self.probes.remove(index);
        }
        self.probes.push(Probe {
            input,
            dirty,
            due,
            used: now,
        });
    }

    /// Queue an explicit user action against the tracked checkout.
    pub fn start(&mut self, action: Action, token: Option<Arc<SecretString>>) -> crate::Result<()> {
        if self.running.is_some() {
            return Err(Error::GitBusy);
        }
        let input = self.input.clone().ok_or(Error::GitNoCheckout)?;
        if let Action::Commit(message) = &action
            && (message.trim().is_empty() || message.len() > MESSAGE_LIMIT)
        {
            return Err(Error::GitCommitMessage);
        }
        if matches!(action, Action::CreatePullRequest) && token.is_none() {
            return Err(Error::GitHubAuthentication);
        }
        self.error = None;
        self.outcome = None;
        self.running = Some(action.clone());
        self.waiting = Some((input, Job::Run(action, token)));
        Ok(())
    }

    pub fn poll(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if let Some(worker) = &self.worker {
            match worker.results.try_recv() {
                Ok((input, completion)) => {
                    self.busy = false;
                    changed = true;
                    match completion {
                        // A status of a checkout the chrome no longer shows is
                        // dropped rather than painted over the current one.
                        Completion::Status(_) if self.input.as_ref() != Some(&input) => {}
                        Completion::Status(Ok(status)) => {
                            self.status = Some(status);
                            self.error = None;
                            self.due = Some(now + REFRESH);
                        }
                        Completion::Status(Err(error)) => {
                            self.status = None;
                            if self.running.is_none() {
                                self.error = Some(error.to_string());
                            }
                            self.due = Some(now + ERROR_BACKOFF);
                        }
                        Completion::Dirty(result) => {
                            self.record_probe(input, result.ok(), now);
                        }
                        Completion::Action(result) => {
                            self.running = None;
                            match result {
                                Ok(outcome) => self.outcome = Some(outcome),
                                Err(error) => self.error = Some(error.to_string()),
                            }
                            // A commit or push changed the tree it reported on.
                            if self.input.as_ref() == Some(&input) {
                                self.due = Some(now);
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.busy = false;
                    self.waiting = None;
                    self.worker = None;
                    self.running = None;
                    self.error = Some(Error::GitWorker.to_string());
                    self.due = Some(now + ERROR_BACKOFF);
                    return true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.busy
            && self.waiting.is_none()
            && let Some(input) = self.queue.pop_front()
        {
            self.waiting = Some((input, Job::Dirty));
        }
        if !self.busy
            && let Some((input, job)) = self.waiting.take()
        {
            if self.worker.is_none() {
                let (requests, incoming) = mpsc::sync_channel::<(u64, Input, Job)>(1);
                let (outgoing, results) = mpsc::sync_channel(1);
                let current = self.generation.clone();
                match thread::Builder::new()
                    .name("herdr-git".into())
                    .spawn(move || {
                        for (generation, input, job) in incoming {
                            let cancelled = || current.load(Ordering::Relaxed) != generation;
                            let completion = execute(&input, job, &cancelled);
                            if outgoing.send((input, completion)).is_err() {
                                break;
                            }
                        }
                    }) {
                    Ok(_) => self.worker = Some(Worker { requests, results }),
                    Err(source) => {
                        self.running = None;
                        self.error = Some(
                            Error::GitProcess {
                                operation: "start the Git worker",
                                source,
                            }
                            .to_string(),
                        );
                        self.due = Some(now + ERROR_BACKOFF);
                        return true;
                    }
                }
            }
            if let Some(worker) = &self.worker {
                let generation = self.generation.load(Ordering::Relaxed);
                match worker.requests.try_send((generation, input, job)) {
                    Ok(()) => self.busy = true,
                    Err(mpsc::TrySendError::Full((_, input, job))) => {
                        self.waiting = Some((input, job))
                    }
                    Err(mpsc::TrySendError::Disconnected(_)) => {
                        self.worker = None;
                        self.running = None;
                        self.error = Some(Error::GitWorker.to_string());
                        self.due = Some(now + ERROR_BACKOFF);
                        changed = true;
                    }
                }
            }
        }
        changed
    }
}

fn execute(input: &Input, job: Job, cancelled: &impl Fn() -> bool) -> Completion {
    match job {
        Job::Status => {
            Completion::Status(status(input, Instant::now() + STATUS_TIMEOUT, cancelled))
        }
        Job::Dirty => Completion::Dirty(dirty(input, Instant::now() + STATUS_TIMEOUT, cancelled)),
        // Writes are never cancelled: killing `git commit` or `git push`
        // halfway through can leave an index lock or a half-written ref behind,
        // so only their own deadline ends them.
        Job::Run(action, token) => Completion::Action(perform(input, action, token, &|| false)),
    }
}

fn status(
    input: &Input,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Status> {
    let checkout = local_checkout(input, deadline, cancelled)?;
    checkout_status(&checkout, deadline, cancelled)
}

/// Porcelain output is machine readable and collapses untracked directories,
/// so an unignored build directory stays one entry rather than thousands.
fn dirty(input: &Input, deadline: Instant, cancelled: &impl Fn() -> bool) -> crate::Result<bool> {
    let checkout = local_checkout(input, deadline, cancelled)?;
    let status = git(
        &checkout,
        &["status", "--porcelain", "--untracked-files=normal", "-z"],
        "read working tree state",
        deadline,
        cancelled,
    )?;
    Ok(!status.is_empty())
}

fn checkout_status(
    checkout: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Status> {
    let numstat = git(
        checkout,
        &["diff", "--numstat", "HEAD"],
        "read working tree changes",
        deadline,
        cancelled,
    )?;
    // Untracked directories collapse to one entry, so an unignored build
    // directory cannot push the listing past the worker's output limit.
    let untracked = git(
        checkout,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--directory",
            "--no-empty-directory",
            "-z",
        ],
        "list untracked files",
        deadline,
        cancelled,
    )?;
    let mut status = parse_numstat(&numstat);
    status.untracked = untracked
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .count() as u64;
    Ok(status)
}

/// `--numstat` is machine readable in any locale; binary files report `-` and
/// contribute no line counts.
fn parse_numstat(text: &str) -> Status {
    let mut status = Status::default();
    for line in text.lines() {
        let mut fields = line.split('\t');
        let additions = fields.next().and_then(|field| field.parse::<u64>().ok());
        let deletions = fields.next().and_then(|field| field.parse::<u64>().ok());
        status.additions = status.additions.saturating_add(additions.unwrap_or(0));
        status.deletions = status.deletions.saturating_add(deletions.unwrap_or(0));
    }
    status
}

fn perform(
    input: &Input,
    action: Action,
    token: Option<Arc<SecretString>>,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Outcome> {
    let deadline = Instant::now() + OPERATION_TIMEOUT;
    let checkout = local_checkout(input, deadline, cancelled)?;
    match action {
        Action::Commit(message) => commit(&checkout, &message, deadline, cancelled),
        Action::Push => push(&checkout, &input.branch, deadline, cancelled),
        Action::CreatePullRequest => {
            let token = token.ok_or(Error::GitHubAuthentication)?;
            create_pull_request(&checkout, &input.branch, &token, deadline, cancelled)
        }
    }
}

fn commit(
    checkout: &str,
    message: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Outcome> {
    if !checkout_status(checkout, deadline, cancelled)?.dirty() {
        return Err(Error::GitNothingToCommit);
    }
    git(
        checkout,
        &["add", "-A"],
        "stage changes",
        deadline,
        cancelled,
    )?;
    git(
        checkout,
        &["commit", "-m", message],
        "commit",
        deadline,
        cancelled,
    )?;
    let head = git(
        checkout,
        &["log", "-1", "--pretty=format:%h %s"],
        "read the new commit",
        deadline,
        cancelled,
    )?;
    Ok(Outcome {
        message: format!("Committed {}", clean(&head)),
        url: None,
    })
}

fn push(
    checkout: &str,
    branch: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Outcome> {
    git(
        checkout,
        &["push", "--set-upstream", "origin", branch],
        "push",
        deadline,
        cancelled,
    )?;
    Ok(Outcome {
        message: format!("Pushed {branch} to origin"),
        url: None,
    })
}

fn create_pull_request(
    checkout: &str,
    branch: &str,
    token: &SecretString,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Outcome> {
    let (owner, repo) = origin_repository(checkout, deadline, cancelled)?;
    let title = clean(&git(
        checkout,
        &["log", "-1", "--pretty=format:%s"],
        "read the commit subject",
        deadline,
        cancelled,
    )?);
    let title: String = title.trim().chars().take(TITLE_LIMIT).collect();
    if title.is_empty() {
        return Err(Error::GitPullRequestTitle);
    }
    // The head ref must exist on GitHub before a pull request can reference it.
    git(
        checkout,
        &["push", "--set-upstream", "origin", branch],
        "push",
        deadline,
        cancelled,
    )?;
    let mut cooldown = None;
    let repository = crate::github::graphql(
        "create_pr_repository",
        token,
        REPOSITORY_QUERY,
        serde_json::json!({"owner": owner, "repo": repo}),
        API_TIMEOUT,
        cancelled,
        &mut cooldown,
    )?;
    let id = repository["data"]["repository"]["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or(Error::PrRepository)?
        .to_owned();
    let base = repository["data"]["repository"]["defaultBranchRef"]["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or(Error::PrRepository)?
        .to_owned();
    if base == branch {
        return Err(Error::GitPullRequestBase);
    }
    let created = crate::github::graphql(
        "create_pr",
        token,
        CREATE_MUTATION,
        serde_json::json!({
            "repository": id, "base": base, "head": branch, "title": title, "body": ""
        }),
        API_TIMEOUT,
        cancelled,
        &mut cooldown,
    )?;
    parse_created(&created, &owner, &repo)
}

fn parse_created(response: &serde_json::Value, owner: &str, repo: &str) -> crate::Result<Outcome> {
    let pr = &response["data"]["createPullRequest"]["pullRequest"];
    let number = pr["number"].as_u64().filter(|number| *number > 0);
    let url = pr["url"].as_str().unwrap_or_default();
    let expected = number.map(|number| format!("https://github.com/{owner}/{repo}/pull/{number}"));
    if expected.as_deref() != Some(url) {
        return Err(Error::PrIdentity);
    }
    Ok(Outcome {
        message: format!("Opened pull request #{}", number.unwrap_or_default()),
        url: expected,
    })
}

/// Every Git child runs through the shared process policy: no shell, no
/// terminal prompts, bounded output, and a deadline the caller owns.
fn git(
    checkout: &str,
    args: &[&str],
    operation: &'static str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<String> {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "core.fsmonitor=false",
            "--no-optional-locks",
            "-C",
            checkout,
        ])
        .args(args);
    let (ok, output) = run(&mut command, deadline, cancelled)?;
    if ok {
        Ok(output.trim_end_matches(['\r', '\n']).to_owned())
    } else {
        Err(Error::GitFailed {
            operation,
            details: clean(output.trim()),
        })
    }
}

/// Fresh close-time probe, including metadata-only and submodule changes that
/// line counts cannot represent. Remote-tracking refs are the local evidence of
/// a push; this read-only check never contacts a remote.
pub(super) fn close_status(
    input: &Input,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<(bool, bool)> {
    let checkout = local_checkout(input, deadline, cancelled)?;
    let dirty = !git(
        &checkout,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=normal",
            "--ignore-submodules=none",
        ],
        "check uncommitted files",
        deadline,
        cancelled,
    )?
    .is_empty();
    let unpushed = !git(
        &checkout,
        &["rev-list", "--max-count=1", "HEAD", "--not", "--remotes"],
        "check unpushed commits",
        deadline,
        cancelled,
    )?
    .is_empty();
    Ok((dirty, unpushed))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn close_probe_detects_untracked_staged_unstaged_and_unpublished_work() {
        let directory = tempfile::tempdir().unwrap();
        let hooks = tempfile::tempdir().unwrap();
        let checkout = directory.path().to_str().unwrap();
        let command = |args: &[&str]| {
            let result = Command::new("git")
                .args([
                    "-C",
                    checkout,
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                ])
                .arg(format!("core.hooksPath={}", hooks.path().display()))
                .args(args)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        };
        command(&["init", "-b", "test"]);
        std::fs::write(directory.path().join("tracked"), "original\n").unwrap();
        command(&["add", "tracked"]);
        command(&["commit", "-m", "initial"]);
        let input = Input {
            repo_key: directory.path().join(".git").to_str().unwrap().to_owned(),
            branch: "test".into(),
            checkout: Some(checkout.into()),
        };
        let probe =
            || close_status(&input, Instant::now() + Duration::from_secs(15), &|| false).unwrap();
        assert_eq!(probe(), (false, true)); // No upstream or remote refs.
        command(&["update-ref", "refs/remotes/origin/test", "HEAD"]);
        assert_eq!(probe(), (false, false));
        std::fs::write(directory.path().join("new"), "new\n").unwrap();
        assert_eq!(probe(), (true, false));
        command(&["add", "new"]);
        assert_eq!(probe(), (true, false));
        command(&["commit", "-m", "unpublished"]);
        assert_eq!(probe(), (false, true));
        std::fs::write(directory.path().join("tracked"), "modified\n").unwrap();
        assert_eq!(probe(), (true, true));
        assert!(matches!(
            close_status(&input, Instant::now() + Duration::from_secs(15), &|| true),
            Err(Error::PrCancelled)
        ));
    }

    struct Peer {
        git: Git,
        incoming: mpsc::Receiver<(u64, Input, Job)>,
        outgoing: mpsc::SyncSender<(Input, Completion)>,
    }

    impl Peer {
        fn new() -> Self {
            let (requests, incoming) = mpsc::sync_channel(1);
            let (outgoing, results) = mpsc::sync_channel(1);
            let mut git = Git::default();
            git.worker = Some(Worker { requests, results });
            Self {
                git,
                incoming,
                outgoing,
            }
        }

        fn request(&mut self) -> (Input, Job) {
            let (_, input, job) = self.incoming.try_recv().unwrap();
            (input, job)
        }

        fn complete(&mut self, input: Input, completion: Completion, now: Instant) {
            self.outgoing.send((input, completion)).unwrap();
            self.git.poll(now);
        }
    }

    fn input(branch: &str) -> Input {
        Input {
            checkout: None,
            repo_key: "/repo/.git".into(),
            branch: branch.into(),
        }
    }

    fn status(additions: u64, deletions: u64, untracked: u64) -> Status {
        Status {
            additions,
            deletions,
            untracked,
        }
    }

    #[test]
    fn numstat_sums_text_changes_and_ignores_binary_rows() {
        assert_eq!(
            parse_numstat("12\t3\tsrc/main.rs\n-\t-\tlogo.png\n0\t7\tREADME.md\n"),
            status(12, 10, 0)
        );
        assert_eq!(parse_numstat(""), Status::default());
        assert!(!Status::default().dirty());
        assert!(status(0, 0, 1).dirty());
    }

    #[test]
    fn status_refreshes_on_a_ttl_and_only_while_the_window_is_active() {
        let mut peer = Peer::new();
        let now = Instant::now();
        assert!(peer.git.track(Some(input("feature")), true, now));
        peer.git.poll(now);
        let (requested, job) = peer.request();
        assert_eq!(requested, input("feature"));
        assert!(matches!(job, Job::Status));
        peer.complete(requested, Completion::Status(Ok(status(9, 2, 1))), now);
        assert_eq!(peer.git.status(), Some(status(9, 2, 1)));
        let early = now + REFRESH - Duration::from_secs(1);
        peer.git.track(Some(input("feature")), true, early);
        peer.git.poll(early);
        assert!(
            peer.incoming.try_recv().is_err(),
            "refresh waits for the TTL"
        );
        let due = now + REFRESH;
        peer.git.track(Some(input("feature")), false, due);
        peer.git.poll(due);
        assert!(
            peer.incoming.try_recv().is_err(),
            "an inactive window does not poll Git"
        );
        assert_eq!(
            peer.git.status(),
            Some(status(9, 2, 1)),
            "and keeps its last status"
        );
        peer.git.track(Some(input("feature")), true, due);
        peer.git.poll(due);
        assert!(matches!(peer.request().1, Job::Status));
    }

    #[test]
    fn a_changed_checkout_drops_its_predecessors_status() {
        let mut peer = Peer::new();
        let now = Instant::now();
        peer.git.track(Some(input("feature")), true, now);
        peer.git.poll(now);
        let (requested, _) = peer.request();
        assert!(peer.git.track(Some(input("other")), true, now));
        assert_eq!(peer.git.status(), None);
        peer.complete(requested, Completion::Status(Ok(status(9, 2, 0))), now);
        assert_eq!(
            peer.git.status(),
            None,
            "a stale checkout cannot paint over the new one"
        );
        peer.git.track(Some(input("other")), true, now);
        peer.git.poll(now);
        let (requested, _) = peer.request();
        assert_eq!(requested, input("other"));
        peer.complete(requested, Completion::Status(Ok(status(1, 1, 0))), now);
        assert_eq!(peer.git.status(), Some(status(1, 1, 0)));
        assert!(peer.git.track(None, true, now));
        assert_eq!(peer.git.status(), None);
    }

    #[test]
    fn one_action_runs_at_a_time_and_reports_its_own_result() {
        let mut peer = Peer::new();
        let now = Instant::now();
        peer.git.track(Some(input("feature")), true, now);
        assert!(matches!(
            peer.git.start(Action::Commit("  ".into()), None),
            Err(Error::GitCommitMessage)
        ));
        assert!(matches!(
            peer.git.start(Action::CreatePullRequest, None),
            Err(Error::GitHubAuthentication)
        ));
        peer.git
            .start(Action::Commit("subject".into()), None)
            .unwrap();
        assert!(matches!(
            peer.git.start(Action::Push, None),
            Err(Error::GitBusy)
        ));
        peer.git.poll(now);
        let (requested, job) = peer.request();
        assert!(matches!(job, Job::Run(Action::Commit(message), None) if message == "subject"));
        // Switching workspaces mid-commit must not cancel or misreport it.
        peer.git.track(Some(input("other")), true, now);
        peer.complete(
            requested,
            Completion::Action(Ok(Outcome {
                message: "Committed abc1234 subject".into(),
                url: None,
            })),
            now,
        );
        assert_eq!(
            peer.git.outcome().map(|outcome| outcome.message.as_str()),
            Some("Committed abc1234 subject")
        );
        assert!(peer.git.running().is_none());
        peer.git.start(Action::Push, None).unwrap();
        peer.git.poll(now);
        let (requested, _) = peer.request();
        peer.complete(
            requested,
            Completion::Action(Err(Error::GitFailed {
                operation: "push",
                details: "rejected".into(),
            })),
            now,
        );
        assert!(peer.git.outcome().is_none());
        assert!(
            peer.git
                .error()
                .is_some_and(|error| error.contains("rejected"))
        );
    }

    #[test]
    fn listed_checkouts_are_probed_round_robin_and_forgotten_when_they_go() {
        let mut peer = Peer::new();
        let now = Instant::now();
        let listed = [input("feature"), input("other")];
        peer.git.track(Some(input("feature")), true, now);
        peer.git.track_listed(listed.clone(), true, now);
        peer.git.poll(now);
        // The focused checkout is counted by its own status, not probed twice.
        let (requested, job) = peer.request();
        assert_eq!(requested, input("feature"));
        assert!(matches!(job, Job::Status));
        peer.complete(requested, Completion::Status(Ok(status(1, 0, 0))), now);
        assert_eq!(peer.git.dirty("/repo/.git", "feature"), Some(true));
        peer.git.poll(now);
        let (requested, job) = peer.request();
        assert_eq!(requested, input("other"), "the rest are probed in turn");
        assert!(matches!(job, Job::Dirty));
        assert_eq!(
            peer.git.dirty("/repo/.git", "other"),
            None,
            "unknown, not clean"
        );
        peer.complete(requested, Completion::Dirty(Ok(true)), now);
        assert_eq!(peer.git.dirty("/repo/.git", "other"), Some(true));
        // A failed probe is remembered as unknown and backs off further.
        peer.git
            .track_listed(listed.clone(), true, now + PROBE_REFRESH);
        peer.git.poll(now + PROBE_REFRESH);
        let (requested, _) = peer.request();
        peer.complete(
            requested,
            Completion::Dirty(Err(Error::GitNoCheckout)),
            now + PROBE_REFRESH,
        );
        assert_eq!(peer.git.dirty("/repo/.git", "other"), None);
        let early = now + PROBE_REFRESH + PROBE_BACKOFF - Duration::from_secs(1);
        peer.git.track_listed(listed.clone(), true, early);
        peer.git.poll(early);
        assert!(peer.incoming.try_recv().is_err(), "a failure waits longer");
        let due = now + PROBE_REFRESH + PROBE_BACKOFF;
        peer.git.track_listed(listed.clone(), false, due);
        peer.git.poll(due);
        assert!(
            peer.incoming.try_recv().is_err(),
            "an inactive window probes nothing"
        );
        peer.git.track_listed(listed, true, due);
        peer.git.poll(due);
        assert!(matches!(peer.request().1, Job::Dirty));
        // A workspace that leaves the listing takes its answer with it.
        peer.git
            .track_listed([input("feature")], true, due + SCAN_INTERVAL);
        assert_eq!(peer.git.dirty("/repo/.git", "other"), None);
        assert!(peer.git.queue.is_empty());
    }

    #[test]
    fn the_probe_cache_is_bounded() {
        let mut git = Git::default();
        let now = Instant::now();
        let listed: Vec<_> = (0..CACHE_LIMIT + 10)
            .map(|index| input(&format!("branch-{index}")))
            .collect();
        git.track_listed(listed.clone(), true, now);
        assert_eq!(git.queue.len(), CACHE_LIMIT);
        for (index, input) in listed.iter().enumerate() {
            git.seed_probe(input.clone(), index.is_multiple_of(2), now);
        }
        assert_eq!(git.probes.len(), CACHE_LIMIT);
    }

    #[test]
    fn a_stopped_worker_is_reported_and_does_not_strand_an_action() {
        let mut peer = Peer::new();
        let now = Instant::now();
        peer.git.track(Some(input("feature")), true, now);
        peer.git.start(Action::Push, None).unwrap();
        peer.git.poll(now);
        let _ = peer.request();
        drop(peer.outgoing);
        assert!(peer.git.poll(now));
        assert!(peer.git.running().is_none());
        assert_eq!(
            peer.git.error(),
            Some(Error::GitWorker.to_string().as_str())
        );
    }

    #[test]
    fn created_pull_requests_must_match_their_repository() {
        let response = |url: &str, number: u64| serde_json::json!({"data":{"createPullRequest":{"pullRequest":{"number":number,"url":url}}}});
        let outcome = parse_created(
            &response("https://github.com/example/project/pull/7", 7),
            "example",
            "project",
        )
        .unwrap();
        assert_eq!(outcome.message, "Opened pull request #7");
        assert_eq!(
            outcome.url.as_deref(),
            Some("https://github.com/example/project/pull/7")
        );
        for (url, number) in [
            ("https://github.com/other/project/pull/7", 7),
            ("https://github.com/example/project/pull/8", 7),
            ("https://example.com/example/project/pull/7", 7),
            ("https://github.com/example/project/pull/0", 0),
        ] {
            assert!(matches!(
                parse_created(&response(url, number), "example", "project"),
                Err(Error::PrIdentity)
            ));
        }
    }
}
