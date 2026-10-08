//! Keeps the Mac running with its lid closed while the cup is on.
//!
//! Only `pmset disablesleep` overrides lid sleep, and it needs root, so a
//! sudoers rule limited to `pmset disablesleep 1` and `0` is installed once
//! through the system's own password dialog. The override outlives this
//! process, so a watcher child owns undoing it: it runs `pmset disablesleep 0`
//! once its stdin closes. Turning the cup off closes it, and so do quitting
//! and crashing, because the kernel closes the pipe with the process.
//!
//! `disablesleep` is one system-wide flag, so a flag something else already
//! set is left as found: Herdr neither claims it nor clears it.

use super::Caffeine;
use crate::{Error, Result};
use gpui::App;
use std::process::{Child, Command, Stdio};

/// Shown by the system's password dialog when the rule is installed.
const PASSWORD_PROMPT: &str = "Herdr wants to keep this Mac running with the lid closed.";
const RULE_PATH: &str = "/etc/sudoers.d/herdr-gpui";
/// Failure diagnostics keep the start of the command's stderr, bounded.
const DETAIL_LIMIT: usize = 200;

#[derive(Default)]
pub(super) struct Lid {
    /// Whether the cup is on with the closed-lid preference set.
    wanted: bool,
    state: State,
    commands: Commands,
}

/// A failed step: a refused hold means the preference no longer describes
/// what the Mac does, so the caller turns it off.
pub(super) enum Failure {
    Hold(Error),
    Release(Error),
}

impl Lid {
    pub(super) fn held(&self) -> bool {
        matches!(self.state, State::Held(_) | State::Found)
    }
}

#[derive(Default)]
enum State {
    #[default]
    Allowed,
    /// A command runs in the background; `sync` resumes once it settles.
    Busy,
    /// Sleep is disabled, and the watcher undoes it when its stdin closes.
    Held(Child),
    /// Sleep was already disabled when the cup wanted it; whoever disabled it
    /// owns restoring it.
    Found,
}

/// The programs and arguments run for each step, so tests substitute
/// harmless ones.
#[derive(Clone)]
struct Commands {
    /// Prints the power settings, `SleepDisabled 1` among them when set.
    sleep_settings: Vec<String>,
    /// Succeeds only when the sudoers rule lets `disable_sleep` run.
    allowed: Vec<String>,
    disable_sleep: Vec<String>,
    install_rule: Vec<String>,
    watcher: Vec<String>,
}

impl Default for Commands {
    fn default() -> Self {
        let install = format!(
            "printf '%s\\n' '{rule}' > {RULE_PATH} && chmod 0440 {RULE_PATH} \
             && /usr/sbin/visudo -c -f {RULE_PATH} || {{ rm -f {RULE_PATH}; exit 1; }}",
            rule = rule(uid()),
        );
        Self {
            sleep_settings: ["/usr/bin/pmset", "-g"].map(String::from).into(),
            allowed: [
                "/usr/bin/sudo",
                "-n",
                "-l",
                "/usr/bin/pmset",
                "disablesleep",
                "1",
            ]
            .map(String::from)
            .into(),
            disable_sleep: ["/usr/bin/sudo", "-n", "/usr/bin/pmset", "disablesleep", "1"]
                .map(String::from)
                .into(),
            install_rule: vec![
                "/usr/bin/osascript".into(),
                "-e".into(),
                format!(
                    "do shell script {install:?} with administrator privileges with prompt {PASSWORD_PROMPT:?}"
                ),
            ],
            // Signals are ignored so only the closed pipe ends the wait, even
            // when a terminal or logout signals the whole group.
            watcher: vec![
                "/bin/sh".into(),
                "-c".into(),
                "trap '' HUP INT TERM; read line; exec /usr/bin/sudo -n /usr/bin/pmset disablesleep 0"
                    .into(),
            ],
        }
    }
}

/// Records whether sleep should stay disabled, and starts moving toward it.
pub(super) fn want(
    wanted: bool,
    report: impl Fn(Failure, &mut App) + Clone + 'static,
    cx: &mut App,
) {
    cx.default_global::<Caffeine>().lid.wanted = wanted;
    sync(report, cx);
}

/// Runs at most one command at a time; whatever changed meanwhile is picked
/// up when it settles.
fn sync(report: impl Fn(Failure, &mut App) + Clone + 'static, cx: &mut App) {
    let lid = &mut cx.default_global::<Caffeine>().lid;
    let commands = lid.commands.clone();
    let work = match (std::mem::take(&mut lid.state), lid.wanted) {
        (State::Allowed, true) => cx.background_executor().spawn(async move {
            hold(&commands)
                .map(|watcher| watcher.map_or(State::Found, State::Held))
                .map_err(Failure::Hold)
        }),
        (State::Held(watcher), false) => cx.background_executor().spawn(async move {
            release(watcher)
                .map(|()| State::Allowed)
                .map_err(Failure::Release)
        }),
        (State::Found, false) => {
            cx.refresh_windows();
            return;
        }
        (state, _) => {
            lid.state = state;
            return;
        }
    };
    cx.default_global::<Caffeine>().lid.state = State::Busy;
    cx.spawn(async move |cx| {
        let outcome = work.await;
        cx.update(|cx| {
            let lid = &mut cx.default_global::<Caffeine>().lid;
            let failure = match outcome {
                Ok(state) => {
                    lid.state = state;
                    None
                }
                Err(failure) => {
                    // A refused hold is not retried until the cup or the
                    // preference changes again.
                    lid.state = State::Allowed;
                    if matches!(failure, Failure::Hold(_)) {
                        lid.wanted = false;
                    }
                    Some(failure)
                }
            };
            cx.refresh_windows();
            if let Some(failure) = failure {
                report.clone()(failure, cx);
            }
            sync(report, cx);
        });
    })
    .detach();
}

/// Disables sleep, installing the sudoers rule first if it does not allow
/// `pmset` yet, and returns the watcher that restores it. Sleep that is
/// already disabled returns no watcher and is left alone. The watcher starts
/// before anything changes, so a crash at any point still restores sleep.
fn hold(commands: &Commands) -> Result<Option<Child>> {
    if sleep_disabled(&commands.sleep_settings)? {
        return Ok(None);
    }
    let watcher = command(&commands.watcher)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| Error::LidCommand {
            operation: "start the closed-lid watcher",
            source,
        })?;
    let held = allow(commands).and_then(|()| {
        run(
            &commands.disable_sleep,
            "keep the Mac running with the lid closed",
        )
    });
    match held {
        Ok(()) => Ok(Some(watcher)),
        Err(error) => {
            // Restoring sleep that was never disabled is harmless.
            let _ = release(watcher);
            Err(error)
        }
    }
}

/// Installs the sudoers rule unless `sudo` already allows `pmset`. Only a
/// refusal asks for the password; a `sudo` that cannot run at all fails here
/// instead of prompting for a rule that would not help.
fn allow(commands: &Commands) -> Result<()> {
    match run(&commands.allowed, "check the closed-lid sudoers rule") {
        Err(Error::LidCommandFailed { .. }) => {
            run(&commands.install_rule, "allow Herdr to change lid sleep").map_err(cancelled)
        }
        checked => checked,
    }
}

/// Whether `pmset -g` reports `SleepDisabled 1`; the line is absent while
/// sleep is allowed.
fn sleep_disabled(argv: &[String]) -> Result<bool> {
    const OPERATION: &str = "read the Mac's sleep settings";
    let output = command(argv)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|source| Error::LidCommand {
            operation: OPERATION,
            source,
        })?;
    if !output.status.success() {
        return Err(Error::LidCommandFailed {
            operation: OPERATION,
            status: output.status,
            detail: String::new(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        let mut fields = line.split_whitespace();
        fields.next() == Some("SleepDisabled") && fields.next() == Some("1")
    }))
}

/// The password dialog's Cancel button exits `osascript` with AppleScript's
/// userCanceledErr, -128; anything else stays a command failure.
fn cancelled(error: Error) -> Error {
    match error {
        Error::LidCommandFailed { ref detail, .. } if detail.ends_with("(-128)") => {
            Error::LidPasswordCancelled
        }
        error => error,
    }
}

/// Closes the watcher's stdin and waits for its `pmset disablesleep 0`.
fn release(mut watcher: Child) -> Result<()> {
    drop(watcher.stdin.take());
    let status = watcher.wait().map_err(|source| Error::LidCommand {
        operation: "wait for the closed-lid watcher",
        source,
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::LidRelease(status))
    }
}

fn run(argv: &[String], operation: &'static str) -> Result<()> {
    let output = command(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|source| Error::LidCommand { operation, source })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(Error::LidCommandFailed {
        operation,
        status: output.status,
        detail: stderr.trim().chars().take(DETAIL_LIMIT).collect(),
    })
}

fn command(argv: &[String]) -> Command {
    let mut command = Command::new(argv.first().map_or("", String::as_str));
    command.args(argv.iter().skip(1));
    command
}

/// The sudoers entry, keyed by uid so no user name needs escaping; sudoers
/// reads `#501` as uid 501, not as a comment.
fn rule(uid: u32) -> String {
    format!(
        "#{uid} ALL=(root) NOPASSWD: /usr/bin/pmset disablesleep 1, /usr/bin/pmset disablesleep 0"
    )
}

#[cfg(unix)]
fn uid() -> u32 {
    rustix::process::getuid().as_raw()
}

/// The cup is macOS-only, so no rule is ever installed elsewhere.
#[cfg(not(unix))]
fn uid() -> u32 {
    0
}

#[cfg(all(test, unix))]
mod tests;
