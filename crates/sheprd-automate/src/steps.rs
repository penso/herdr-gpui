//! What each step does on the host. A step never errors out of a run: any
//! failure, from a dirty checkout to Sheprd not answering, becomes a failed
//! step with the reason at the end of its log.

use crate::{
    fingerprint::WorktreeScripts,
    herdr::Herdr,
    paths::{Paths, private_dir, private_file},
    process::{self, Job},
    runner::Reservation,
    store::{AgentRow, Store},
};
use sheprd_automation::{Resolved, Step};
use std::{
    cell::Cell,
    ffi::OsStr,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

/// Why a step failed before or instead of running its command.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the checkout has uncommitted changes; the step does not stash or reset")]
    Dirty,
    #[error("git {command}: {stderr}")]
    Git {
        command: &'static str,
        stderr: String,
    },
    #[error(transparent)]
    Herdr(#[from] crate::herdr::Error),
    #[error("worktree scripts need a Unix shell")]
    NeedsUnixShell,
    #[error("{} has no {name} script", path.display())]
    NoWorktreeScript { path: PathBuf, name: String },
    #[error("a personal script has no repository; move it to a repository's .herdr/automations")]
    NoRepository,
    #[error(transparent)]
    Run(#[from] crate::Error),
    #[error(
        "a command that uses an earlier step's output needs a Unix shell; cmd cannot keep it from being read as commands"
    )]
    TokensNeedUnixShell,
    #[error("the checkout is on {current}, not {wanted}; the step does not switch branches")]
    WrongBranch { current: String, wanted: String },
}

/// Where and for whom a step runs.
#[derive(Clone, Copy)]
pub struct StepContext<'a> {
    pub paths: &'a Paths,
    pub store: &'a Store,
    pub herdr: &'a Herdr,
    pub run: i64,
    pub script_path: &'a str,
    pub script_name: &'a str,
    /// The repository a repository script runs against.
    pub repo: Option<&'a Path>,
    /// Its worktree scripts as approved and read when the run started.
    pub worktree: Option<&'a WorktreeScripts>,
    pub stop: &'a Arc<AtomicBool>,
    /// The host slot the runner reserved for an agent step, released once
    /// the agent is recorded and counts itself.
    pub reserved: &'a Cell<Option<Reservation>>,
    pub now: fn() -> i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepResult {
    pub ok: bool,
    pub exit: Option<i32>,
    pub log_tail: String,
    pub worktree: Option<String>,
    pub pane: Option<String>,
    /// What `{run_id:N}` fills in with: the agent's pane.
    pub run_id: Option<String>,
    pub cpu_ms: Option<u64>,
    pub peak_memory: Option<u64>,
}

/// Runs step `position` (from 1), given at most `timeout`.
pub fn execute(
    context: &StepContext<'_>,
    step: &Resolved,
    position: u32,
    timeout: Duration,
) -> StepResult {
    let log = context.paths.step_log(context.run, position);
    // Herdr calls end with the step too, as its commands do.
    let herdr = context
        .herdr
        .bounded(context.stop.clone(), Instant::now() + timeout);
    let context = &StepContext {
        herdr: &herdr,
        ..*context
    };
    let result = match &step.call.step {
        Step::Pull { branch } => pull(context, &log, branch, timeout),
        Step::Run { name } => worktree_script(context, &log, name, timeout),
        Step::Sh { command } => shell(context, &log, command, &step.env, timeout),
        Step::Worktree { name, base } => worktree(context, name, base.as_deref()),
        Step::Agent {
            harness,
            prompt,
            worktree,
            base,
            wait,
        } => agent(
            context,
            position,
            harness,
            prompt,
            worktree.as_deref(),
            base.as_deref(),
            wait.then_some(timeout),
        ),
        Step::Notify { text } => notify(context, text),
    };
    match result {
        Ok(mut result) => {
            if result.log_tail.is_empty() {
                result.log_tail = process::tail(&log, process::TAIL_BYTES)
                    .map(|(text, _)| text)
                    .unwrap_or_default();
            }
            result
        }
        Err(error) => {
            // The display boundary: the reason becomes the step's log.
            let reason = error.to_string();
            note(&log, &reason);
            StepResult {
                log_tail: process::tail(&log, process::TAIL_BYTES)
                    .map(|(text, _)| text)
                    .unwrap_or(reason),
                ..StepResult::default()
            }
        }
    }
}

/// Appends a line to a step's log, best effort.
fn note(log: &Path, line: &str) {
    if let Some(parent) = log.parent() {
        let _ = private_dir(parent);
    }
    if let Ok(mut file) = private_file().create(true).append(true).open(log) {
        let _ = writeln!(file, "{line}");
    }
}

type Outcome = Result<StepResult, Error>;

fn repo<'a>(context: &StepContext<'a>) -> Result<&'a Path, Error> {
    context.repo.ok_or(Error::NoRepository)
}

fn finished(job: &Job<'_>) -> Outcome {
    let finished = process::run(job)?;
    if finished.timed_out {
        note(
            job.log,
            "herdr-automate: the step ran past its timeout and was stopped",
        );
    }
    if finished.stopped {
        note(job.log, "herdr-automate: the run was stopped");
    }
    Ok(StepResult {
        ok: finished.ok(),
        exit: finished.exit,
        log_tail: process::tail(job.log, process::TAIL_BYTES)
            .map(|(text, _)| text)
            .unwrap_or_default(),
        cpu_ms: finished.cpu_ms,
        peak_memory: finished.peak_memory,
        ..StepResult::default()
    })
}

/// Fast-forwards `branch` in the main checkout. Refuses a dirty checkout or
/// one on another branch rather than stash, reset or switch.
fn pull(context: &StepContext<'_>, log: &Path, branch: &str, timeout: Duration) -> Outcome {
    let repo = repo(context)?;
    let git = |command: &'static str, args: &[&str]| -> Result<String, Error> {
        let output = sheprd_client::new_command("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .map_err(crate::Error::io("git"))?;
        if !output.status.success() {
            return Err(Error::Git {
                command,
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let current = git("symbolic-ref", &["symbolic-ref", "--short", "HEAD"])?;
    if current != branch {
        return Err(Error::WrongBranch {
            current,
            wanted: branch.to_owned(),
        });
    }
    if !git("status", &["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        return Err(Error::Dirty);
    }
    let args = [
        OsStr::new("-C"),
        repo.as_os_str(),
        OsStr::new("pull"),
        OsStr::new("--ff-only"),
    ];
    finished(&Job {
        program: OsStr::new("git"),
        args: &args,
        cwd: Some(repo),
        env: &[],
        log,
        timeout,
        stop: context.stop,
    })
}

/// The `setup` or `run` script of the repository's `.herdr/worktree.toml`,
/// as approved and read when the run started, run as Sheprd runs it.
fn worktree_script(
    context: &StepContext<'_>,
    log: &Path,
    name: &str,
    timeout: Duration,
) -> Outcome {
    let repo = repo(context)?;
    let script = context
        .worktree
        .and_then(|scripts| scripts.get(name))
        .ok_or_else(|| Error::NoWorktreeScript {
            path: repo.join(".herdr").join("worktree.toml"),
            name: name.to_owned(),
        })?;
    if cfg!(windows) {
        return Err(Error::NeedsUnixShell);
    }
    let env = [
        ("HERDR_WORKTREE_SCRIPT".to_owned(), script.to_owned()),
        (
            "HERDR_WORKTREE_PATH".to_owned(),
            repo.to_string_lossy().into_owned(),
        ),
    ];
    let args = [
        OsStr::new("-ec"),
        OsStr::new("eval \"$HERDR_WORKTREE_SCRIPT\""),
    ];
    finished(&Job {
        program: OsStr::new("/bin/sh"),
        args: &args,
        cwd: Some(repo),
        env: &env,
        log,
        timeout,
        stop: context.stop,
    })
}

fn shell(
    context: &StepContext<'_>,
    log: &Path,
    command: &str,
    env: &[(String, String)],
    timeout: Duration,
) -> Outcome {
    // `cmd` expands `%NAME%` before it parses the line, so a value would
    // be read as commands: refuse rather than hand it over.
    if cfg!(windows) && !env.is_empty() {
        return Err(Error::TokensNeedUnixShell);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let cwd = context.repo.map(Path::to_path_buf).or(home);
    let mut variables = env.to_vec();
    variables.push((
        "HERDR_AUTOMATION".to_owned(),
        context.script_name.to_owned(),
    ));
    variables.push(("HERDR_AUTOMATION_RUN".to_owned(), context.run.to_string()));
    let (program, args): (&OsStr, [&OsStr; 2]) = if cfg!(windows) {
        (OsStr::new("cmd"), [OsStr::new("/C"), OsStr::new(command)])
    } else {
        (
            OsStr::new("/bin/sh"),
            [OsStr::new("-c"), OsStr::new(command)],
        )
    };
    finished(&Job {
        program,
        args: &args,
        cwd: cwd.as_deref(),
        env: &variables,
        log,
        timeout,
        stop: context.stop,
    })
}

fn worktree(context: &StepContext<'_>, name: &str, base: Option<&str>) -> Outcome {
    let repo = repo(context)?;
    context.herdr.ensure_server()?;
    let created = context.herdr.create_worktree(repo, name, base)?;
    Ok(StepResult {
        ok: true,
        log_tail: format!("created worktree {} on {name}", created.path),
        worktree: Some(created.path),
        pane: Some(created.pane),
        ..StepResult::default()
    })
}

fn agent(
    context: &StepContext<'_>,
    position: u32,
    harness: &str,
    prompt: &str,
    worktree: Option<&str>,
    base: Option<&str>,
    wait: Option<Duration>,
) -> Outcome {
    let repo = repo(context)?;
    context.herdr.ensure_server()?;
    let branch = worktree.map(str::to_owned).unwrap_or_else(|| {
        format!(
            "automate/{}-{}-{position}",
            context.script_name, context.run
        )
    });
    let created = context.herdr.create_worktree(repo, &branch, base)?;
    let label = format!("{}-{}", context.script_name, context.run);
    context.herdr.start_agent(&label, harness, &created.pane)?;
    let row = AgentRow {
        run: context.run,
        position,
        path: context.script_path.to_owned(),
        harness: harness.to_owned(),
        pane: created.pane.clone(),
        cwd: created.path.clone(),
        started_at: (context.now)(),
        ..AgentRow::default()
    };
    context.store.add_agent(&row)?;
    drop(context.reserved.take());
    context.herdr.prompt(&created.pane, prompt, wait)?;
    let waited = if wait.is_some() {
        ", and it finished"
    } else {
        ""
    };
    Ok(StepResult {
        ok: true,
        log_tail: format!(
            "started {harness} in pane {} at {} and sent the prompt{waited}",
            created.pane, created.path
        ),
        worktree: Some(created.path),
        pane: Some(created.pane.clone()),
        run_id: Some(created.pane),
        ..StepResult::default()
    })
}

fn notify(context: &StepContext<'_>, text: &str) -> Outcome {
    context.herdr.ensure_server()?;
    context.herdr.notify(context.script_name, text)?;
    Ok(StepResult {
        ok: true,
        log_tail: format!("notified: {text}"),
        ..StepResult::default()
    })
}
