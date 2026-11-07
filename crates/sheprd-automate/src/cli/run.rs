//! Carrying out a parsed command. Each command builds an answer, a text
//! form and a JSON form, and one place prints it, so errors look the same
//! everywhere: `error: ...` as text, `{"ok": false, "message": ...}` as
//! JSON. Only `serve`, `watch` and `log --follow` stream instead.

use super::{Command, LogTarget, RepoAction, Target, watch};
use crate::{
    Error, Result,
    approve::{self, Mode},
    daemon,
    herdr::Herdr,
    paths::Paths,
    process, report,
    runner::now,
    scan, service,
    settings::Settings,
    store::{RequestKind, Store, Switch},
};
use serde::Serialize;
use serde_json::{Value, json};
use sheprd_automation::{
    Kind, Limits, Plan, Source,
    report::{Done, LogText, MAX_LOG_BYTES, RunSummary},
};
use std::{
    fmt::Write as _,
    io::{self, Write},
    path::Path,
    process::ExitCode,
};

/// What a command answers.
struct Answer {
    text: String,
    json: Value,
    code: ExitCode,
}

impl Answer {
    fn of(text: impl Into<String>, json: &impl Serialize) -> Result<Self> {
        Ok(Self {
            text: text.into(),
            json: serde_json::to_value(json)?,
            code: ExitCode::SUCCESS,
        })
    }

    fn done(message: impl Into<String>, run: Option<i64>) -> Result<Self> {
        let message = message.into();
        Self::of(
            message.clone(),
            &Done {
                ok: true,
                message,
                run,
            },
        )
    }
}

/// Runs `command` against this user's state.
pub fn run(command: &Command, out: &mut impl Write) -> io::Result<ExitCode> {
    match Paths::from_env() {
        Ok(paths) => run_with(command, &paths, out, true),
        Err(error) => print_error(command, &error, out),
    }
}

/// Runs `command` against the state under `paths`, never starting the
/// daemon.
#[cfg(test)]
pub fn run_in(command: &Command, paths: &Paths, out: &mut impl Write) -> io::Result<ExitCode> {
    run_with(command, paths, out, false)
}

/// Runs `command` against the state under `paths`. With `start_daemon`, a
/// command that leaves scripts to run starts the daemon when none is
/// running; tests leave it off, since their executable is the test runner.
pub fn run_with(
    command: &Command,
    paths: &Paths,
    out: &mut impl Write,
    start_daemon: bool,
) -> io::Result<ExitCode> {
    let streamed = match command {
        Command::Serve => Some(daemon::serve(paths).map(|()| ExitCode::SUCCESS)),
        Command::Watch => Some(watch::run(paths, out).map(|()| ExitCode::SUCCESS)),
        Command::Log {
            target: LogTarget::Daemon { lines },
            follow: true,
            ..
        } => Some(watch::follow(&paths.log(), *lines, out).map(|()| ExitCode::SUCCESS)),
        _ => None,
    };
    if let Some(result) = streamed {
        return match result {
            Ok(code) => Ok(code),
            Err(error) => print_error(command, &error, out),
        };
    }
    match answer(command, paths) {
        Ok(mut answer) => {
            if start_daemon {
                ensure_daemon(command, paths, &mut answer);
            }
            if json(command) {
                writeln!(out, "{}", answer.json)?;
            } else if !answer.text.is_empty() {
                writeln!(out, "{}", answer.text.trim_end())?;
            }
            Ok(answer.code)
        }
        Err(error) => print_error(command, &error, out),
    }
}

/// Starts the daemon after a command that leaves scripts to run, when
/// none is running: an approved script, a run asked for, a repository
/// with scripts. Says so in the answer; a start that fails is reported
/// there too, and the command still succeeded.
fn ensure_daemon(command: &Command, paths: &Paths, answer: &mut Answer) {
    if !leaves_scripts_to_run(command) || daemon::Lock::held(paths) {
        return;
    }
    let note = match std::env::current_exe()
        .map_err(Error::io("herdr-automate"))
        .and_then(|exe| service::start(&exe, paths))
    {
        Ok(_) => "; started the daemon".to_owned(),
        Err(error) => format!("; the daemon did not start: {error}"),
    };
    answer.text.push_str(&note);
    if let Some(message) = answer
        .json
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        answer.json["message"] = Value::String(format!("{message}{note}"));
    }
}

/// Whether, once `command` succeeds, there are scripts for a daemon to run.
pub(super) fn leaves_scripts_to_run(command: &Command) -> bool {
    match command {
        Command::Approve { .. } | Command::RunNow { .. } => true,
        Command::Repo {
            action: RepoAction::Add(repo),
            ..
        } => std::fs::read_dir(crate::paths::repository_scripts(repo)).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "luau")
            })
        }),
        _ => false,
    }
}

fn print_error(command: &Command, error: &Error, out: &mut impl Write) -> io::Result<ExitCode> {
    if json(command) {
        let done = Done {
            ok: false,
            message: error.to_string(),
            run: None,
        };
        writeln!(out, "{}", json!(done))?;
    } else {
        writeln!(out, "error: {error}")?;
    }
    Ok(ExitCode::FAILURE)
}

fn json(command: &Command) -> bool {
    match command {
        Command::Approve { json, .. }
        | Command::Check { json, .. }
        | Command::Disable { json, .. }
        | Command::Enable { json, .. }
        | Command::InstallService { json }
        | Command::Log { json, .. }
        | Command::Pause { json, .. }
        | Command::Repo { json, .. }
        | Command::Resume { json, .. }
        | Command::RunNow { json, .. }
        | Command::RunShow { json, .. }
        | Command::Runs { json, .. }
        | Command::Shutdown { json }
        | Command::Start { json }
        | Command::Status { json }
        | Command::Steps { json }
        | Command::Stop { json, .. }
        | Command::UninstallService { json }
        | Command::Usage { json, .. } => *json,
        Command::Help | Command::Serve | Command::Version | Command::Watch => false,
    }
}

fn answer(command: &Command, paths: &Paths) -> Result<Answer> {
    match command {
        Command::Help => Answer::of(super::HELP, &Value::Null),
        Command::Version => Answer::of(
            format!("sheprd-automate {}", daemon::version()),
            &Value::Null,
        ),
        Command::Steps { .. } => {
            let mut text = String::new();
            for kind in Kind::ALL {
                let _ = writeln!(
                    text,
                    "{}\n    {}\n    {}",
                    kind.name(),
                    kind.usage(),
                    kind.summary()
                );
            }
            let steps = Kind::ALL
                .iter()
                .map(|kind| json!({ "name": kind.name(), "usage": kind.usage(), "summary": kind.summary() }))
                .collect::<Vec<_>>();
            Answer::of(text, &json!({ "steps": steps }))
        }
        Command::Check { file, .. } => check(file),
        _ => with_store(command, paths),
    }
}

fn check(file: &Path) -> Result<Answer> {
    let source = Source::read(file)?;
    let plan = Plan::build(&source, Limits::default())?;
    let count = plan.triggers.len();
    let text = format!(
        "{plan}\nOK: {count} trigger{}. Every trigger can also be started by hand.\nsha256 {}",
        if count == 1 { "" } else { "s" },
        plan.hash
    );
    Answer::of(text, &json!({ "ok": true, "plan": plan }))
}

fn with_store(command: &Command, paths: &Paths) -> Result<Answer> {
    let store = Store::open(&paths.database())?;
    let settings = Settings::read(&paths.settings())?;
    match command {
        Command::Approve { file, hash, .. } => {
            let herdr = Herdr::new(settings.herdr.clone(), settings.herdr_session.clone());
            let mode = hash.as_deref().map_or(Mode::Terminal, Mode::Hash);
            let pane = std::env::var("HERDR_PANE_ID")
                .ok()
                .filter(|pane| !pane.is_empty());
            let approved = approve::approve(paths, &store, &herdr, file, mode, pane.as_deref())?;
            let short = approved.hash.get(..12).unwrap_or(&approved.hash);
            Answer::done(
                format!("approved {} (sha256 {short}) on this host", approved.name),
                None,
            )
        }
        Command::Status { .. } => {
            let status = report::status(paths, &store, &settings, now())?;
            Answer::of(watch::render(&status), &status)
        }
        Command::Runs { script, .. } => {
            let path = script
                .as_deref()
                .map(|script| known(&store, script))
                .transpose()?;
            let runs = store
                .runs(path.as_deref(), sheprd_automation::report::RECENT_RUNS)?
                .iter()
                .map(|run| report::summary(&store, run))
                .collect::<Result<Vec<_>>>()?;
            Answer::of(
                runs.iter().map(run_line).collect::<Vec<_>>().join("\n"),
                &runs,
            )
        }
        Command::RunShow { id, .. } => {
            let detail = report::run_detail(&store, *id)?;
            let mut text = run_line(&detail.run);
            for step in &detail.steps {
                let _ = write!(
                    text,
                    "\n  {}. [{:?}] {}",
                    step.position, step.state, step.summary
                );
                for line in step
                    .log_tail
                    .lines()
                    .rev()
                    .take(5)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                {
                    let _ = write!(text, "\n       {line}");
                }
            }
            Answer::of(text, &detail)
        }
        Command::Log { target, .. } => {
            let log = log(paths, &store, target)?;
            Answer::of(log.text.clone(), &log)
        }
        Command::RunNow { file, trigger, .. } => {
            let path = known(&store, file)?;
            let script = store
                .script(&path)?
                .ok_or_else(|| Error::NoScript(file.clone()))?;
            if store.approved_hash(&path)?.as_deref() != Some(script.hash.as_str()) {
                return Err(Error::NotApproved(script.name));
            }
            store.push_request(
                &RequestKind::RunNow {
                    path,
                    trigger: trigger.clone(),
                },
                now(),
            )?;
            let message = if daemon::Lock::held(paths) {
                format!("{} starts now; see `sheprd-automate runs`", script.name)
            } else {
                format!(
                    "{} is queued, but the daemon is not running: `sheprd-automate start`",
                    script.name
                )
            };
            Answer::done(message, None)
        }
        Command::Pause { target, .. } | Command::Resume { target, .. } => {
            let pause = matches!(command, Command::Pause { .. });
            let verb = if pause { "paused" } else { "resumed" };
            match target {
                Target::All => {
                    store.set_paused_all(pause)?;
                    Answer::done(format!("scheduling {verb} on this host"), None)
                }
                Target::Script(file) => {
                    let path = known(&store, file)?;
                    let switch = store.switch(&path)?;
                    store.set_switch(
                        &path,
                        &Switch {
                            paused: pause,
                            reason: pause.then(|| "paused by you".to_owned()),
                            ..switch
                        },
                    )?;
                    if !pause {
                        // Fire nothing for the time it was paused.
                        store.clear_cursors(&path)?;
                    }
                    Answer::done(format!("{} {verb}", name(&store, &path)?), None)
                }
            }
        }
        Command::Disable { file, .. } | Command::Enable { file, .. } => {
            let disable = matches!(command, Command::Disable { .. });
            let path = known(&store, file)?;
            let switch = store.switch(&path)?;
            store.set_switch(
                &path,
                &Switch {
                    disabled: disable,
                    reason: disable.then(|| "disabled by you".to_owned()),
                    ..switch
                },
            )?;
            if !disable {
                store.clear_cursors(&path)?;
            }
            let verb = if disable { "disabled" } else { "enabled" };
            Answer::done(format!("{} {verb}", name(&store, &path)?), None)
        }
        Command::Stop { run, .. } => {
            let row = store.run(*run)?.ok_or(Error::NoRun(*run))?;
            if row.state != sheprd_automation::report::RunState::Running {
                return Err(Error::NotRunning(*run));
            }
            store.push_request(&RequestKind::Stop { run: *run }, now())?;
            Answer::done(format!("asked run {run} to stop"), Some(*run))
        }
        Command::Repo { action, .. } => repo(&store, action),
        Command::Usage {
            script, since, by, ..
        } => {
            let path = script
                .as_deref()
                .map(|script| known(&store, script))
                .transpose()?;
            let usage = report::usage(&store, path.as_deref(), since.start(now()), *by)?;
            Answer::of(watch::render_usage(&usage), &usage)
        }
        Command::Start { .. } => {
            let exe = std::env::current_exe().map_err(Error::io("herdr-automate"))?;
            let message = if service::start(&exe, paths)? {
                "the daemon is already running"
            } else {
                "the daemon started"
            };
            Answer::done(message, None)
        }
        Command::Shutdown { .. } => {
            let message = if service::shutdown(paths, &store)? {
                "the daemon stopped"
            } else {
                "the daemon was not running"
            };
            Answer::done(message, None)
        }
        Command::InstallService { .. } => {
            let exe = std::env::current_exe().map_err(Error::io("herdr-automate"))?;
            let described = service::describe(&exe, paths)?;
            service::install(&described, paths)?;
            let mut text = String::new();
            if let (Some(file), Some(content)) = (&described.file, &described.content) {
                let _ = writeln!(text, "wrote {}:\n{content}", file.display());
            }
            for command in &described.commands {
                let _ = writeln!(text, "ran: {}", command.join(" "));
            }
            let _ = write!(
                text,
                "the daemon now runs at login and restarts after a crash"
            );
            let done = Done {
                ok: true,
                message: "installed the service".to_owned(),
                run: None,
            };
            Answer::of(text, &done)
        }
        Command::UninstallService { .. } => {
            service::uninstall(paths)?;
            Answer::done(
                "removed the service; the daemon no longer starts at login",
                None,
            )
        }
        Command::Help
        | Command::Version
        | Command::Steps { .. }
        | Command::Check { .. }
        | Command::Serve
        | Command::Watch => Answer::of("", &Value::Null),
    }
}

/// A script path as the store knows it.
fn known(store: &Store, file: &Path) -> Result<String> {
    let canonical = std::fs::canonicalize(file).map_err(Error::io(file))?;
    let path = canonical.to_string_lossy().into_owned();
    match store.script(&path)? {
        Some(_) => Ok(path),
        None => Err(Error::NoScript(canonical)),
    }
}

fn name(store: &Store, path: &str) -> Result<String> {
    Ok(store
        .script(path)?
        .map(|script| script.name)
        .unwrap_or_else(|| path.to_owned()))
}

fn run_line(run: &RunSummary) -> String {
    let detail = run
        .detail
        .as_deref()
        .map(|detail| format!(" ({detail})"))
        .unwrap_or_default();
    let tokens = run
        .tokens
        .map(|tokens| format!(", {tokens} tokens"))
        .unwrap_or_default();
    format!(
        "#{} {} [{:?}, {:?}] {} step(s), {} agent(s){tokens}{detail}",
        run.id, run.name, run.reason, run.state, run.steps, run.agents
    )
}

fn log(paths: &Paths, store: &Store, target: &LogTarget) -> Result<LogText> {
    let limit = u64::try_from(MAX_LOG_BYTES).unwrap_or(u64::MAX);
    match target {
        LogTarget::Daemon { lines } => {
            let (text, truncated) =
                process::tail_lines(&paths.log(), limit, *lines).unwrap_or_default();
            Ok(LogText { text, truncated })
        }
        LogTarget::Run { id, step } => {
            store.run(*id)?.ok_or(Error::NoRun(*id))?;
            let steps = store.steps(*id)?;
            let mut text = String::new();
            let mut truncated = false;
            for row in steps
                .iter()
                .filter(|row| step.is_none_or(|step| step == row.position))
            {
                let (body, cut) =
                    process::tail_lines(&paths.step_log(*id, row.position), limit, usize::MAX)
                        .unwrap_or_else(|_| (row.log_tail.clone(), false));
                truncated |= cut;
                if step.is_none() {
                    let _ = writeln!(text, "== step {}: {} ==", row.position, row.summary);
                }
                let _ = writeln!(text, "{body}");
            }
            if text.len() > MAX_LOG_BYTES {
                let mut start = text.len() - MAX_LOG_BYTES;
                while !text.is_char_boundary(start) {
                    start += 1;
                }
                text = text.split_off(start);
                truncated = true;
            }
            Ok(LogText { text, truncated })
        }
    }
}

fn repo(store: &Store, action: &RepoAction) -> Result<Answer> {
    match action {
        RepoAction::List => {
            let repos = store.repos()?;
            Answer::of(repos.join("\n"), &repos)
        }
        RepoAction::Add(path) => {
            let repo = std::fs::canonicalize(path).map_err(Error::io(path))?;
            if !repo.join(".git").exists() {
                return Err(Error::NotRepository(repo));
            }
            let text = repo.to_string_lossy().into_owned();
            let added = store.add_repo(&text, now())?;
            let scripts = scan::repository_scripts_hint(&repo);
            Answer::done(
                if added {
                    format!("added {text}; scripts go in {scripts}")
                } else {
                    format!("{text} was already added")
                },
                None,
            )
        }
        RepoAction::Remove(path) => {
            let text = std::fs::canonicalize(path)
                .map(|repo| repo.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            let message = if store.remove_repo(&text)? {
                format!("removed {text}")
            } else {
                format!("{text} was not added")
            };
            Answer::done(message, None)
        }
    }
}
