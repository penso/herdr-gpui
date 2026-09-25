//! Fetching a PR: the GraphQL query, the Git commands that identify the
//! checkout, and the bounded subprocess policy they all run under. Output is
//! size-capped and every call has a deadline, so no step can hang the worker.

use super::{Input, Result, parse_graphql};
use crate::Error;
#[cfg(unix)]
use std::os::{fd::OwnedFd, unix::net::UnixStream};
use std::{
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;
pub(super) const TIMEOUT: Duration = Duration::from_secs(15);
const QUERY: &str = r#"query($owner: String!, $repo: String!, $branch: String!) {
  repository(owner: $owner, name: $repo) {
    pullRequests(first: 2, headRefName: $branch, orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes {
        number url title state isDraft headRefName baseRefName additions deletions
        changedFiles updatedAt mergeStateStatus reviewDecision headRepositoryOwner { login }
        commits(last: 1) { nodes { commit { statusCheckRollup {
          contexts(first: 100) {
            pageInfo { hasNextPage }
            nodes { __typename ... on CheckRun { status conclusion } ... on StatusContext { state } }
          }
        } } } }
      }
    }
  }
}"#;

#[cfg(test)]
pub(super) fn fetch(
    input: &Input,
    token: &secrecy::SecretString,
    cancelled: impl Fn() -> bool,
) -> Result {
    fetch_with_backoff(input, token, cancelled, &mut None)
}

pub(super) fn fetch_with_backoff(
    input: &Input,
    token: &secrecy::SecretString,
    cancelled: impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> Result {
    let deadline = Instant::now() + TIMEOUT;
    let (owner, repo) = local_repository(input, deadline, &cancelled)?;
    let timeout = deadline
        .checked_duration_since(Instant::now())
        .ok_or(Error::PrTimeout)?;
    let response = crate::github::graphql(
        "pull_request",
        token,
        QUERY,
        serde_json::json!({"owner":owner,"repo":repo,"branch":input.branch}),
        timeout,
        cancelled,
        cooldown,
    )?;
    parse_graphql(response, &owner, &repo, &input.branch)
}

pub(crate) fn local_repository(
    input: &Input,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<(String, String)> {
    let checkout = local_checkout(input, deadline, cancelled)?;
    origin_repository(&checkout, deadline, cancelled)
}

/// The GitHub owner and repository behind a verified checkout's origin remote.
pub(crate) fn origin_repository(
    checkout: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<(String, String)> {
    let remote = git(
        checkout,
        &["config", "--get", "remote.origin.url"],
        deadline,
        cancelled,
    )?;
    crate::avatars::github_repo(&remote).ok_or_else(|| {
        // An SSH host alias for a second account (`github-work:owner/repo`)
        // is the usual reason; only the host is logged, never credentials.
        tracing::debug!(
            category = "github_origin",
            host = remote_host(&remote),
            "Origin remote is not a GitHub.com repository"
        );
        Error::PrOrigin
    })
}

/// The host of a Git remote, without the user, credentials, or path.
pub(super) fn remote_host(remote: &str) -> &str {
    let authority = match remote.split_once("://") {
        Some((_, rest)) => rest.split('/').next().unwrap_or_default(),
        None => remote.split(':').next().unwrap_or_default(),
    };
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
}

/// Resolve the checkout a daemon workspace names and verify it still is that
/// repository on that branch. Every local Git operation starts here, so a
/// renamed branch or a moved worktree cannot be worked on by mistake.
pub(crate) fn local_checkout(
    input: &Input,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<String> {
    if input
        .checkout
        .as_ref()
        .is_some_and(|path| !Path::new(path).is_absolute())
        || !Path::new(&input.repo_key).is_absolute()
    {
        return Err(Error::PrAbsolutePath);
    }
    if input.branch.is_empty()
        || input.branch.len() > 1024
        || input.branch.chars().any(char::is_control)
    {
        return Err(Error::PrBranch);
    }
    let checkout = match &input.checkout {
        Some(path) => path.clone(),
        None => {
            // Older daemons lack workspace.get. Use Git's own worktree registry,
            // never pane cwd or the daemon's new-workspace directory policy.
            let mut command = Command::new("git");
            command.args([
                "-c",
                "core.fsmonitor=false",
                "--git-dir",
                &input.repo_key,
                "worktree",
                "list",
                "--porcelain",
                "-z",
            ]);
            let (ok, output) = run(&mut command, deadline, cancelled)?;
            if !ok {
                return Err(Error::PrWorktreeLookup);
            }
            worktree_checkout(&output, &input.branch)?
        }
    };
    // A Git registry candidate still must match both repository and live HEAD.
    let common = git(
        &checkout,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        deadline,
        cancelled,
    )?;
    if Path::new(&common)
        .canonicalize()
        .ok()
        .zip(Path::new(&input.repo_key).canonicalize().ok())
        .is_none_or(|(actual, expected)| actual != expected)
    {
        return Err(Error::PrRepositoryMismatch);
    }
    if git(
        &checkout,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        deadline,
        cancelled,
    )? != input.branch
    {
        return Err(Error::PrBranchChanged);
    }
    Ok(checkout)
}

/// Read-only Git output from a checkout, with the shared process policy.
fn git(
    checkout: &str,
    args: &[&str],
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<String> {
    let mut command = Command::new("git");
    command
        .args(["-c", "core.fsmonitor=false", "-C", checkout])
        .args(args);
    run(&mut command, deadline, cancelled).and_then(|(ok, output)| {
        if ok {
            Ok(output.trim_end_matches(['\r', '\n']).to_owned())
        } else {
            Err(Error::PrCheckout)
        }
    })
}

pub(super) fn worktree_checkout(output: &str, branch: &str) -> crate::Result<String> {
    let branch = format!("branch refs/heads/{branch}");
    let mut paths = output.split("\0\0").filter_map(|record| {
        let mut fields = record.split('\0');
        let path = fields.next()?.strip_prefix("worktree ")?;
        (Path::new(path).is_absolute() && fields.any(|field| field == branch)).then_some(path)
    });
    let path = paths.next().ok_or(Error::PrMissingWorktree)?;
    if paths.next().is_some() {
        return Err(Error::PrAmbiguousWorktree);
    }
    Ok(path.into())
}

pub(crate) fn run(
    command: &mut Command,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<(bool, String)> {
    if cancelled() {
        return Err(Error::PrCancelled);
    }
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .current_dir("/")
        .env_remove("GH_REPO")
        .env_remove("GH_DEBUG")
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env("GH_HOST", "github.com")
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null());
    let (output, mut child) = capture(command)?;
    // Command retains Stdio descriptors after spawn; release them so EOF is observable.
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let result = collect(output, &mut child, deadline, cancelled);
    if result.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    result
}

fn spawned(source: std::io::Error) -> Error {
    Error::PrProcess {
        operation: "launch Git (install git on PATH)",
        source,
    }
}

fn unreadable(source: std::io::Error) -> Error {
    Error::PrProcess {
        operation: "read process output",
        source,
    }
}

/// Merges the child's stdout and stderr into one stream this process can poll.
#[cfg(unix)]
fn capture(command: &mut Command) -> crate::Result<(UnixStream, Child)> {
    let (reader, writer) = UnixStream::pair().map_err(|source| Error::PrProcess {
        operation: "create process output channel",
        source,
    })?;
    reader
        .set_nonblocking(true)
        .map_err(|source| Error::PrProcess {
            operation: "configure process output",
            source,
        })?;
    let error_writer = writer.try_clone().map_err(|source| Error::PrProcess {
        operation: "configure process errors",
        source,
    })?;
    command
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::from(OwnedFd::from(error_writer)));
    let child = command.spawn().map_err(spawned)?;
    Ok((reader, child))
}

/// Windows cannot hand a socket to a child as its standard streams, so the two
/// halves of the output join in an anonymous pipe instead.
#[cfg(windows)]
fn capture(command: &mut Command) -> crate::Result<(std::io::PipeReader, Child)> {
    let (reader, writer) = std::io::pipe().map_err(|source| Error::PrProcess {
        operation: "create process output channel",
        source,
    })?;
    let error_writer = writer.try_clone().map_err(|source| Error::PrProcess {
        operation: "configure process errors",
        source,
    })?;
    command
        .stdout(Stdio::from(writer))
        .stderr(Stdio::from(error_writer));
    let child = command.spawn().map_err(spawned)?;
    Ok((reader, child))
}

/// Reads the merged output under the caller's deadline and cancellation. The
/// child is only reaped once its output has ended, so nothing is truncated.
#[cfg(unix)]
fn collect(
    mut reader: UnixStream,
    child: &mut Child,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<(bool, String)> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    let mut eof = false;
    loop {
        if cancelled() {
            return Err(Error::PrCancelled);
        }
        if Instant::now() >= deadline {
            return Err(Error::PrTimeout);
        }
        match reader.read(&mut buffer) {
            Ok(0) => eof = true,
            Ok(n) => {
                if output.len() + n > OUTPUT_LIMIT {
                    return Err(Error::PrSize);
                }
                output.extend_from_slice(&buffer[..n]);
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(source) => return Err(unreadable(source)),
        }
        if let Some(status) = child.try_wait().map_err(|source| Error::PrProcess {
            operation: "wait for process",
            source,
        })? && eof
        {
            return String::from_utf8(output)
                .map(|text| (status.success(), text))
                .map_err(|error| Error::PrEncoding(error.utf8_error()));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// An anonymous pipe on Windows cannot be made nonblocking, so the reads run on
/// their own thread and the deadline is enforced here. Killing the child closes
/// the last writer, which ends that thread.
#[cfg(windows)]
fn collect(
    mut reader: std::io::PipeReader,
    child: &mut Child,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<(bool, String)> {
    let (sender, reads) = std::sync::mpsc::channel();
    thread::Builder::new()
        .name("herdr-pr-output".into())
        .spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0; 8192];
            let result = loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break Ok(output),
                    Ok(n) => {
                        if output.len() + n > OUTPUT_LIMIT {
                            break Err(Error::PrSize);
                        }
                        output.extend_from_slice(&buffer[..n]);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(source) => break Err(unreadable(source)),
                }
            };
            let _ = sender.send(result);
        })
        .map_err(unreadable)?;
    let mut ended: Option<Vec<u8>> = None;
    loop {
        if cancelled() {
            return Err(Error::PrCancelled);
        }
        if Instant::now() >= deadline {
            return Err(Error::PrTimeout);
        }
        if ended.is_none() {
            match reads.try_recv() {
                Ok(result) => ended = Some(result?),
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(unreadable(std::io::Error::other("output reader stopped")));
                }
            }
        }
        if let Some(status) = child.try_wait().map_err(|source| Error::PrProcess {
            operation: "wait for process",
            source,
        })? && let Some(output) = ended.take()
        {
            return String::from_utf8(output)
                .map(|text| (status.success(), text))
                .map_err(|error| Error::PrEncoding(error.utf8_error()));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
