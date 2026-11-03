//! Running `herdr-automate COMMAND --json` on a host and reading its answer.
//! Everything here blocks, so it runs on the background executor only.
//!
//! The daemon's command line is the contract (`herdr_automation::report`):
//! one JSON document on stdout, exit 0 on success, and on failure exit 1
//! with a [`Done`] whose `message` says why. A shell's 127, or a program
//! this machine cannot find, means the host has no `herdr-automate`. The
//! output is untrusted: it is bounded, parsed strictly into the report
//! types, and every string in a status or run report is stripped of control
//! characters and cut to a bound once, here ([`super::clean`]), so drawing
//! never sees raw daemon text.

use super::Error;
use crate::teleport::Host;
use herdr_automation::report::{Done, LogText, PROTOCOL, RunDetail, Status};
use herdr_client::{ScriptLimits, shell_quote};
use serde::de::DeserializeOwned;
use std::{io::Read, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

pub(crate) const PROGRAM: &str = "herdr-automate";
/// The most any answer may print; a log is at most 256 KiB before escaping.
pub(crate) const OUTPUT: u64 = 4 * 1024 * 1024;
/// How long a query may go without output.
pub(crate) const IDLE: Duration = Duration::from_secs(30);
/// The longest refusal message kept.
const MESSAGE: usize = 1000;

/// The `herdr-automate` shipped beside this app's executable, if there is one.
pub(crate) fn bundled() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.with_file_name(format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// Runs `herdr-automate` with `args` on `host`, feeding it `input`, and
/// returns what it printed when it exits 0.
pub(crate) fn run(
    host: &Host,
    args: &[&str],
    input: impl Read + Send,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, Error> {
    run_within(host, args, input, IDLE, cancelled)
}

/// [`run`] for a command that may stay quiet longer than a query.
pub(crate) fn run_within(
    host: &Host,
    args: &[&str],
    input: impl Read + Send,
    idle: Duration,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, Error> {
    let limits = ScriptLimits {
        output: OUTPUT,
        idle,
    };
    let mut output = Vec::new();
    let result = if host.is_local() {
        let command = local_command(host, args);
        herdr_client::run_script_command(command, input, &mut output, limits, cancelled)
    } else {
        host.stream(&line(args), input, &mut output, limits, cancelled)
    };
    match result {
        Ok(_) => Ok(output),
        Err(error) => Err(classify(error, &output, host.is_local())),
    }
}

/// `herdr-automate ARGS` on this machine. The app may have been started from
/// a Herdr pane, and `approve` refuses to run inside one (an agent there
/// must not approve its own script), so the pane's ID is not passed on.
pub(crate) fn local_command(host: &Host, args: &[&str]) -> std::process::Command {
    let program = bundled().map_or_else(|| PROGRAM.to_owned(), |path| path.display().to_string());
    let mut command = host.local_command(&program);
    command.args(args).env_remove("HERDR_PANE_ID");
    command
}

/// One script line running `herdr-automate` with `args`, each quoted.
pub(crate) fn line(args: &[&str]) -> String {
    let words: Vec<String> = std::iter::once(PROGRAM)
        .chain(args.iter().copied())
        .map(shell_quote)
        .collect();
    format!("{}\n", words.join(" "))
}

/// A failed run as the section explains it: the binary missing, the
/// program's own reason, or the transport's.
pub(crate) fn classify(error: herdr_client::Error, stdout: &[u8], local: bool) -> Error {
    match &error {
        herdr_client::Error::ScriptExit { status, .. } if status.code() == Some(127) => {
            return Error::Missing;
        }
        herdr_client::Error::ScriptSpawn(source)
            if local && source.kind() == std::io::ErrorKind::NotFound =>
        {
            return Error::Missing;
        }
        herdr_client::Error::ScriptExit { .. } => {
            if let Ok(done) = serde_json::from_slice::<Done>(stdout)
                && !done.ok
            {
                return refused(&done.message);
            }
        }
        _ => {}
    }
    Error::Run(error)
}

fn refused(message: &str) -> Error {
    let message = crate::notifications::safe_text(message, MESSAGE);
    if message.trim().is_empty() {
        Error::Failed
    } else {
        Error::Refused(message)
    }
}

/// Parses one report from `output`.
pub(crate) fn parse<T: DeserializeOwned>(output: &[u8]) -> Result<T, Error> {
    serde_json::from_slice(output).map_err(Error::Json)
}

/// `herdr-automate ARGS --json`, parsed as `T`.
pub(crate) fn query<T: DeserializeOwned>(
    host: &Host,
    args: &[&str],
    cancelled: &AtomicBool,
) -> Result<T, Error> {
    query_within(host, args, IDLE, cancelled)
}

fn query_within<T: DeserializeOwned>(
    host: &Host,
    args: &[&str],
    idle: Duration,
    cancelled: &AtomicBool,
) -> Result<T, Error> {
    let args: Vec<&str> = args.iter().copied().chain(["--json"]).collect();
    parse(&run_within(host, &args, &[][..], idle, cancelled)?)
}

/// The host's scripts, runs, and daemon, refused when it speaks a newer
/// report protocol than this app reads.
pub(crate) fn status(host: &Host, cancelled: &AtomicBool) -> Result<Status, Error> {
    checked(query(host, &["status"], cancelled)?).map(super::clean::status)
}

pub(crate) fn checked(status: Status) -> Result<Status, Error> {
    if status.protocol > PROTOCOL {
        return Err(Error::Protocol {
            found: status.protocol,
            expected: PROTOCOL,
        });
    }
    Ok(status)
}

pub(crate) fn run_detail(
    host: &Host,
    run: i64,
    cancelled: &AtomicBool,
) -> Result<RunDetail, Error> {
    query(host, &["runs", "show", &run.to_string()], cancelled).map(super::clean::detail)
}

/// A run's whole output, or one step's.
pub(crate) fn run_log(
    host: &Host,
    run: i64,
    step: Option<u32>,
    cancelled: &AtomicBool,
) -> Result<LogText, Error> {
    let run = run.to_string();
    let step = step.map(|step| step.to_string());
    let mut args = vec!["log", run.as_str()];
    if let Some(step) = &step {
        args.extend(["--step", step.as_str()]);
    }
    query(host, &args, cancelled)
}

/// The end of the daemon's own log, at most `lines` lines.
pub(crate) fn daemon_log(
    host: &Host,
    lines: u32,
    cancelled: &AtomicBool,
) -> Result<LogText, Error> {
    let lines = lines.to_string();
    query(host, &["log", "--daemon", "--lines", &lines], cancelled)
}

/// A command that changes something; its message when it worked.
pub(crate) fn act(host: &Host, args: &[&str], cancelled: &AtomicBool) -> Result<Done, Error> {
    act_within(host, args, IDLE, cancelled)
}

/// [`act`] for a command that may stay quiet a while, such as `shutdown`
/// waiting for runs to finish.
pub(crate) fn act_within(
    host: &Host,
    args: &[&str],
    idle: Duration,
    cancelled: &AtomicBool,
) -> Result<Done, Error> {
    let done: Done = query_within(host, args, idle, cancelled)?;
    if !done.ok {
        return Err(refused(&done.message));
    }
    Ok(Done {
        message: crate::notifications::safe_text(&done.message, MESSAGE),
        ..done
    })
}

/// The version `herdr-automate --version` prints after its name.
pub(crate) fn version(host: &Host, cancelled: &AtomicBool) -> Result<String, Error> {
    let output = run(host, &["--version"], &[][..], cancelled)?;
    parse_version(&output)
}

pub(crate) fn parse_version(output: &[u8]) -> Result<String, Error> {
    let text = String::from_utf8_lossy(output);
    text.trim()
        .strip_prefix(PROGRAM)
        .map(str::trim)
        .filter(|version| {
            !version.is_empty()
                && version.len() <= 64
                && version
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        })
        .map(str::to_owned)
        .ok_or_else(|| Error::Version(crate::notifications::safe_text(text.trim(), MESSAGE)))
}
