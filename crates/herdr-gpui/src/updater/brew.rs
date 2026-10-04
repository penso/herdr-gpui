//! Homebrew-cask delegation. A cask-managed installation is owned by Homebrew:
//! replacing the bundle in place would leave `brew list --cask --versions`
//! reporting a version that is no longer installed, and the next `brew upgrade`
//! would overwrite the self-installed app. So the app runs Homebrew instead.
//!
//! Homebrew verifies the cask's own SHA-256 and updates its receipts, but this
//! path deliberately does not run the signed-manifest or designated-requirement
//! checks in `install`: trust moves to Homebrew and the tap. Detection therefore
//! has to prove the cask really owns this exact bundle before delegating.
//!
//! Homebrew trashes the running bundle while upgrading it, so bundle resources
//! may be gone until the user restarts. Restart is offered as soon as the
//! upgrade lands, and the upgrade itself is never interrupted: killing Homebrew
//! mid-move can leave no installed app at all.

use super::error::{Result, UpdateError as Error};
use super::release;
use std::{
    env,
    ffi::OsString,
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const TOKEN: &str = "herdr-gpui";
const BUNDLE: &str = "Herdr.app";
/// Homebrew's two standard prefixes. `PATH` is not consulted: a writable
/// directory earlier in a user's `PATH` must not decide what the app executes.
const PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];
const UPGRADE: Duration = Duration::from_secs(30 * 60);
const QUERY: Duration = Duration::from_secs(60);
const RELAUNCH: Duration = Duration::from_secs(30);
const DETAIL: usize = 120;
/// Homebrew's output is unbounded; the diagnostics built from it are not.
const TAIL: usize = 8;
/// How long to keep draining the pipes after the process exits.
const DRAIN: Duration = Duration::from_secs(5);

pub(super) struct Cask {
    brew: PathBuf,
    prefix: PathBuf,
    bundle: PathBuf,
    home: OsString,
}

/// Executables and their directories must not be writable by anyone but their
/// owner, who must be this user or root.
fn trusted(path: &Path, uid: u32, directory: bool) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path).map_err(Error::Io)?;
    if meta.file_type().is_symlink()
        || meta.is_dir() != directory
        || (meta.uid() != uid && meta.uid() != 0)
        || meta.mode() & 0o022 != 0
    {
        return Err(Error::UnsafeBrew(path.to_owned()));
    }
    Ok(meta)
}

/// Some(cask) only when Homebrew's own records point at this exact bundle.
/// A missing or unrelated Homebrew is not an error: the app is then standalone.
pub(super) fn detect(bundle: &Path, uid: u32) -> Option<Cask> {
    let home = env::var_os("HOME")?;
    let prefixes = env::var_os("HOMEBREW_PREFIX")
        .map(PathBuf::from)
        .into_iter()
        .chain(PREFIXES.iter().map(PathBuf::from));
    locate(bundle, uid, home, prefixes)
}

/// Prefixes and HOME are arguments, not environment reads, so this stays
/// testable without mutating process-wide state.
fn locate(
    bundle: &Path,
    uid: u32,
    home: OsString,
    prefixes: impl IntoIterator<Item = PathBuf>,
) -> Option<Cask> {
    for prefix in prefixes {
        let brew = prefix.join("bin/brew");
        let owns = trusted(&prefix, uid, true).is_ok()
            && trusted(&brew, uid, false).is_ok_and(|meta| meta.mode() & 0o111 != 0)
            && owned_bundle(&prefix, bundle);
        if owns {
            return Some(Cask {
                brew,
                prefix,
                bundle: bundle.to_owned(),
                home,
            });
        }
    }
    None
}

/// Homebrew moves an `app` artifact to its target and leaves a symlink to it in
/// the Caskroom. That link, not a version string, is what proves ownership.
fn owned_bundle(prefix: &Path, bundle: &Path) -> bool {
    let caskroom = prefix.join("Caskroom").join(TOKEN);
    let Ok(entries) = fs::read_dir(&caskroom) else {
        return false;
    };
    entries.filter_map(std::result::Result::ok).any(|entry| {
        let link = entry.path().join(BUNDLE);
        fs::symlink_metadata(&link).is_ok_and(|meta| meta.file_type().is_symlink())
            && fs::read_link(&link).is_ok_and(|target| target == bundle)
    })
}

fn command(cask: &Cask) -> Command {
    let mut command = Command::new(&cask.brew);
    // Homebrew needs a usable environment, so it cannot be cleared entirely.
    // Pass only what it requires, and never inherit the user's PATH.
    command
        .env_clear()
        .env("HOME", &cask.home)
        .env(
            "PATH",
            format!(
                "{}:/usr/bin:/bin:/usr/sbin:/sbin",
                cask.prefix.join("bin").display()
            ),
        )
        .env("LC_ALL", "C")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_COLOR", "1")
        .env("HOMEBREW_NO_EMOJI", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        // No terminal is attached, so anything that wants an answer must fail
        // rather than wait for one that can never arrive.
        .stdin(Stdio::null());
    command
}

/// One progress line: bounded, and stripped of control bytes because it is
/// rendered directly. Homebrew output is data, never markup or escapes.
fn detail(line: &str) -> Option<String> {
    let text: String = line
        .chars()
        .filter(|c| !c.is_control())
        .take(DETAIL)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Keep the last few lines for diagnostics, reporting each one exactly once.
fn record(tail: &mut Vec<String>, text: String, progress: &mut impl FnMut(String)) {
    if tail.len() == TAIL {
        tail.remove(0);
    }
    tail.push(text.clone());
    progress(text);
}

/// Run Homebrew, reporting progress as it goes.
///
/// `install::output` cannot serve here: it clears the environment, caps at 30
/// seconds, and only reads a pipe once it closes, so it can neither run an
/// upgrade nor show that one is progressing.
fn run(
    mut command: Command,
    deadline: Duration,
    cancel: Option<&AtomicBool>,
    mut progress: impl FnMut(String),
) -> Result<Vec<String>> {
    if deadline.is_zero() {
        return Err(Error::BrewTimeout);
    }
    let start = Instant::now();
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Error::Io)?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(Error::MissingValidationPipes);
    };
    let (sender, receiver) = mpsc::channel();
    for pipe in [
        Box::new(stdout) as Box<dyn std::io::Read + Send>,
        Box::new(stderr),
    ] {
        let sender = sender.clone();
        thread::spawn(move || {
            for line in BufReader::new(pipe).lines() {
                // A reader that stops early would block Homebrew on a full pipe.
                if sender.send(line.ok()).is_err() {
                    return;
                }
            }
        });
    }
    drop(sender);
    // Only the tail is retained: Homebrew output is unbounded, diagnostics are not.
    let mut tail: Vec<String> = Vec::new();
    // What ended the run, not yet why: a failure's detail is the last line the
    // process produced, which the drain below may not have recovered yet.
    let ended = loop {
        if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
            break Ended::Error(Error::Cancelled);
        }
        if start.elapsed() > deadline {
            break Ended::Error(Error::BrewTimeout);
        }
        while let Ok(line) = receiver.try_recv() {
            let Some(text) = line.as_deref().and_then(detail) else {
                continue;
            };
            record(&mut tail, text, &mut progress);
        }
        match child.try_wait().map_err(Error::Io)? {
            Some(status) if !status.success() => break Ended::Failed(status),
            Some(_) => break Ended::Exited,
            None => thread::sleep(Duration::from_millis(50)),
        }
    };
    if !matches!(ended, Ended::Exited) {
        let _ = child.kill();
    }
    let _ = child.wait();
    // The child exits before its readers have necessarily forwarded everything,
    // so waiting on the readers is what drains the pipes: try_recv sees an empty
    // channel and discards output still in flight, which loses the version line
    // installed() parses and the lines these diagnostics are built from. Both
    // readers hold a sender and the original was dropped above, so the channel
    // disconnects once they reach EOF. Bounded, because a grandchild that
    // inherited the pipes can hold them open after Homebrew itself exits, and a
    // lost line must not become a hung update.
    //
    // These lines are reported like any other. A short command can exit with
    // its whole output still in flight, and whether a line reaches the caller
    // must not depend on which side of the exit it was read on.
    let drain = Instant::now() + DRAIN;
    loop {
        let remaining = drain.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let Ok(line) = receiver.recv_timeout(remaining) else {
            break;
        };
        let Some(text) = line.as_deref().and_then(detail) else {
            continue;
        };
        record(&mut tail, text, &mut progress);
    }
    match ended {
        Ended::Exited => Ok(tail),
        // A command can exit with the line that says why still in the pipe, so
        // the detail is taken from the drained output, not from the race.
        Ended::Failed(status) => Err(Error::BrewFailed {
            status,
            detail: tail.last().cloned().unwrap_or_default(),
        }),
        Ended::Error(error) => Err(error),
    }
}

/// How a run ended, before its output has finished arriving.
enum Ended {
    Exited,
    Failed(std::process::ExitStatus),
    Error(Error),
}

/// The version Homebrew currently records as installed for the cask.
fn installed(cask: &Cask, timeout: Duration) -> Result<String> {
    let mut command = command(cask);
    command.args(["list", "--cask", "--versions", TOKEN]);
    let lines = run(command, timeout.min(QUERY), None, |_| ())?;
    lines
        .iter()
        .rev()
        .find_map(|line| {
            let (name, version) = line.split_once(char::is_whitespace)?;
            (name == TOKEN)
                .then(|| version.split_whitespace().next_back())
                .flatten()
                .map(str::to_owned)
        })
        .ok_or(Error::BrewVersion)
}

/// Upgrade through Homebrew and confirm it reached at least the offered release
/// and is newer than the running app. Refresh stale metadata and retry once.
///
/// Homebrew is not interrupted once it starts, so cancellation is only honoured
/// before the first mutation is spawned. All attempts share one time budget.
pub(super) fn upgrade(
    cask: &Cask,
    current: &str,
    expected: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(String),
) -> Result<String> {
    if cancel.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    let old = release::parse_version(current).ok_or(Error::CurrentVersion)?;
    let offered = release::parse_version(expected).ok_or(Error::ReleaseVersion)?;
    let start = Instant::now();
    let remaining = || UPGRADE.saturating_sub(start.elapsed());
    let mut refreshed = false;
    loop {
        progress(if refreshed {
            "Retrying Homebrew cask upgrade after refresh...".to_owned()
        } else {
            "Asking Homebrew to upgrade the cask...".to_owned()
        });
        if !refreshed && cancel.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let mut upgrade = command(cask);
        // Keep the initial auto-update enabled. On retry the explicit refresh
        // has already run, so do not ask Homebrew to refresh a second time.
        if refreshed {
            upgrade.env("HOMEBREW_NO_AUTO_UPDATE", "1");
        }
        upgrade.args(["upgrade", "--cask", TOKEN]);
        run(upgrade, remaining(), None, &mut progress)?;
        progress("Checking the installed Homebrew cask version...".to_owned());
        let installed = installed(cask, remaining())?;
        let new = release::parse_version(&installed).ok_or(Error::BrewVersion)?;
        if new > old && new >= offered {
            return Ok(installed);
        }
        if refreshed {
            return Err(Error::BrewStale {
                installed,
                current: current.to_owned(),
                expected: expected.to_owned(),
            });
        }
        // A successful upgrade can still use cached metadata or a tap that has
        // not published the offer yet. Command failures never reach this retry.
        progress("Refreshing Homebrew metadata with brew update...".to_owned());
        let mut update = command(cask);
        update.arg("update");
        run(update, remaining(), None, &mut progress)?;
        refreshed = true;
    }
}

/// Start the upgraded app, leaving the caller to quit once it is up. `open -n`
/// launches the new bundle rather than activating this still-running instance.
pub(super) fn relaunch(cask: &Cask) -> Result<()> {
    let mut command = Command::new("/usr/bin/open");
    command
        .env_clear()
        .env("LC_ALL", "C")
        .arg("-n")
        .arg(&cask.bundle)
        .stdin(Stdio::null());
    run(command, RELAUNCH, None, |_| ()).map(|_| ())
}

#[cfg(test)]
mod tests;
