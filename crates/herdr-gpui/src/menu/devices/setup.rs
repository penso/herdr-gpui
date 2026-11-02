//! Delegate provisioning to the installed Herdr CLI. A host that already runs
//! a compatible Herdr is saved without a terminal; anything that needs SSH or
//! installation prompts runs in this app on a terminal that a local workspace
//! shows (see `setup_relay`), so no remote installer policy is duplicated here.
use super::Error;
use crate::Result;
use herdr_client::{Destination, HostKeys, HostProbe, SavedHost};
use std::{
    io::Read,
    process::{Command, ExitStatus, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

/// `machine add` connects, starts the remote server, and verifies it.
const SAVE_TIMEOUT: Duration = Duration::from_secs(120);
/// Enough for the CLI's final error line without retaining remote output.
const SAVE_STDERR_LIMIT: u64 = 8 * 1024;
/// `machine remove` and `machine rename` edit one local file.
const REMOVE_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a terminal setup may run, prompts included, before it is hung up.
#[cfg(unix)]
const SETUP_LIMIT: Duration = Duration::from_secs(15 * 60);

/// Hosts that some window of this process is adding. Herdr's `machine add`
/// neither locks the catalog nor refuses duplicates, so this is what keeps two
/// windows, or two clicks, from adding the same host at once.
static CLAIMS: Mutex<Vec<(Destination, String)>> = Mutex::new(Vec::new());

/// This process is adding one host and session. Dropping it releases the
/// host: the dialog holds it, then the terminal setup it started, which drops
/// it when the setup ends, however it ends.
#[derive(Debug)]
pub(super) struct Claim {
    destination: Destination,
    session: String,
    /// Whether dropping it releases the host; only a test's stand-in does not.
    releases: bool,
}

impl Claim {
    fn acquire(destination: Destination, session: &str) -> Result<Self> {
        let mut claims = CLAIMS.lock().unwrap_or_else(|error| error.into_inner());
        if claims
            .iter()
            .any(|(claimed, claimed_session)| *claimed == destination && claimed_session == session)
        {
            tracing::warn!(
                category = "device_claim",
                claims = claims.len(),
                "Host is already claimed by this process"
            );
            return Err(Error::Adding.into());
        }
        tracing::debug!(category = "device_claim", "Host claimed");
        claims.push((destination.clone(), session.to_owned()));
        Ok(Self {
            destination,
            session: session.to_owned(),
            releases: true,
        })
    }

    /// A claim on a host no other test uses, for tests that never resolve SSH.
    #[cfg(test)]
    pub(super) fn fixture(host: &str) -> Self {
        let destination = Destination {
            user: "test".into(),
            host: host.into(),
            port: 22,
        };
        Self::acquire(destination, "default").unwrap_or(Self {
            destination: Destination {
                user: "test".into(),
                host: host.into(),
                port: 22,
            },
            session: "default".into(),
            releases: false,
        })
    }

    /// The user `ssh` logs in as, which the local configuration supplies when
    /// the target names none.
    pub(super) fn user(&self) -> &str {
        &self.destination.user
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if !self.releases {
            return;
        }
        let mut claims = CLAIMS.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(index) = claims
            .iter()
            .position(|(claimed, session)| *claimed == self.destination && *session == self.session)
        {
            claims.remove(index);
            tracing::debug!(category = "device_claim", "Host released");
        }
    }
}

/// Resolve the request's host, claim it for this process, then check the
/// catalog on disk. Claiming first means a concurrent add in this process sees
/// the claim even before the catalog changes. Blocks on `ssh -G` and the
/// catalog file: call it from the background executor.
pub(super) fn claim(request: &Request) -> Result<Claim> {
    let destination = herdr_client::resolve_destination(&request.target)?;
    let claim = Claim::acquire(destination, &request.session)?;
    ensure_unsaved(request, &claim, &load_catalog()?, resolve)?;
    Ok(claim)
}

/// Claim a saved host while it is removed, so an add of the same host from
/// this process cannot run at the same time. Blocks on `ssh -G`.
pub(super) fn claim_saved(target: &str, session: &str) -> Result<Claim> {
    let destination = herdr_client::resolve_destination(target)?;
    Claim::acquire(destination, session)
}

/// Forget a saved device with `herdr machine remove`. The CLI only edits the
/// local catalog; it never connects, so the host's own Herdr keeps running.
/// Blocks on a process: call it from the background executor.
pub(super) fn remove(id: &str) -> Result<()> {
    remove_with(crate::daemon::executable().as_os_str(), id)
}

fn remove_with(executable: &std::ffi::OsStr, id: &str) -> Result<()> {
    if !herdr_client::valid_profile_id(id) {
        tracing::warn!(category = "device_remove", "Invalid profile ID");
        return Err(Error::NoProfile.into());
    }
    let (status, _, stderr) = run_cli(executable, &["machine", "remove", id], REMOVE_TIMEOUT)?;
    if status.success() {
        return Ok(());
    }
    Err(Error::Remove {
        status,
        detail: last_line(&stderr),
    }
    .into())
}

/// Rename a saved device with `herdr machine rename`, which edits only the
/// local catalog. Blocks on a process: call it from the background executor.
pub(super) fn rename(id: &str, label: &str) -> Result<()> {
    rename_with(crate::daemon::executable().as_os_str(), id, label)
}

fn rename_with(executable: &std::ffi::OsStr, id: &str, label: &str) -> Result<()> {
    if !herdr_client::valid_profile_id(id) {
        return Err(Error::NoProfile.into());
    }
    // The `=` form keeps a label that starts with `-` from reading as a flag.
    let (status, _, stderr) = run_cli(
        executable,
        &["machine", "rename", id, &format!("--label={label}")],
        REMOVE_TIMEOUT,
    )?;
    if status.success() {
        return Ok(());
    }
    Err(Error::Rename {
        status,
        detail: last_line(&stderr),
    }
    .into())
}

/// Check the catalog on disk again, right before a step that saves.
pub(super) fn verify_unsaved(request: &Request, claim: &Claim) -> Result<()> {
    ensure_unsaved(request, claim, &load_catalog()?, resolve)
}

/// Device setup is refused for development catalogs, so this is the catalog
/// `machine add` writes: the terminal command restores the same state root.
fn load_catalog() -> Result<Vec<SavedHost>> {
    let hosts = herdr_client::load_saved_hosts(false)?;
    for host in &hosts {
        tracing::debug!(
            category = "device_catalog",
            id = host.id,
            enabled = host.enabled,
            "Saved host on disk"
        );
    }
    Ok(hosts)
}

fn resolve(target: &str) -> Option<Destination> {
    herdr_client::resolve_destination(target).ok()
}

/// The first saved profile, in catalog order, that reaches the claimed host
/// and session. `machine add` appends, so the first one is the oldest.
fn first_saved<'a>(
    request: &Request,
    claim: &Claim,
    hosts: &'a [SavedHost],
    resolve: impl Fn(&str) -> Option<Destination>,
) -> Option<&'a SavedHost> {
    hosts.iter().find(|host| {
        host.session == request.session
            && (host.target == request.target
                || resolve(&host.target).as_ref() == Some(&claim.destination))
    })
}

fn ensure_unsaved(
    request: &Request,
    claim: &Claim,
    hosts: &[SavedHost],
    resolve: impl Fn(&str) -> Option<Destination>,
) -> Result<()> {
    match first_saved(request, claim, hosts, resolve) {
        Some(host) => {
            tracing::info!(
                category = "device_catalog",
                saved_id = host.id,
                "Host is already saved"
            );
            Err(Error::Exists(host.label.clone()).into())
        }
        None => Ok(()),
    }
}

/// Herdr caps device labels at this many bytes.
const LABEL_LIMIT: usize = 128;

/// The label a device is saved or renamed with. An empty one names the device
/// after its SSH target as typed, such as `user@host` or an address.
pub(super) fn device_label(label: &str, target: &str) -> Result<String> {
    let label = label.trim();
    let label = if label.is_empty() {
        let target = target.trim();
        let target = target.strip_prefix("ssh://").unwrap_or(target);
        // A long target is shortened on a character boundary to fit.
        let mut end = target.len().min(LABEL_LIMIT);
        while !target.is_char_boundary(end) {
            end -= 1;
        }
        &target[..end]
    } else {
        label
    };
    if label.is_empty() || label.len() > LABEL_LIMIT || label.chars().any(char::is_control) {
        return Err(Error::InvalidLabel.into());
    }
    Ok(label.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Request {
    target: String,
    label: String,
    session: String,
    host_keys: HostKeys,
}

impl Request {
    pub(super) fn new(target: &str, label: &str, session: &str) -> Result<Self> {
        let target = target.trim();
        let label = label.trim();
        let session = session.trim();
        if target.is_empty()
            || target.len() > 1024
            || target.starts_with('-')
            || target
                .strip_prefix("ssh://")
                .unwrap_or(target)
                .rsplit_once('@')
                .is_some_and(|(user, _)| user.contains(':'))
            || target.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(Error::InvalidTarget.into());
        }
        let label = device_label(label, target)?;
        let session = if session.is_empty() {
            "default"
        } else {
            session
        };
        if session.len() > 64
            || matches!(session, "." | "..")
            || !session
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err(Error::InvalidSession.into());
        }
        Ok(Self {
            target: target.into(),
            label,
            session: session.into(),
            host_keys: HostKeys::Verify,
        })
    }

    pub(super) fn with_host_keys(self, host_keys: HostKeys) -> Self {
        Self { host_keys, ..self }
    }

    pub(super) fn target(&self) -> &str {
        &self.target
    }

    pub(super) fn label(&self) -> &str {
        &self.label
    }

    /// Whether a saved device connects the same way: Herdr's `machine add` saves
    /// a new profile every time, so the same host would otherwise appear twice.
    pub(super) fn same_host(&self, target: &str, session: &str) -> bool {
        target == self.target && session == self.session
    }

    /// Blocks on SSH: call it from the background executor.
    pub(super) fn probe(&self) -> Result<HostProbe> {
        Ok(herdr_client::probe_host_with(
            &self.target,
            &self.session,
            self.host_keys,
        )?)
    }

    /// Save the form's host key choice for the target, before the device is
    /// saved, so the device's first connection already follows it. A target
    /// that was never skipped leaves the settings untouched.
    fn remember_host_keys(&self) -> Result<()> {
        if self.host_keys == HostKeys::Verify && HostKeys::of(&self.target) == HostKeys::Verify {
            return Ok(());
        }
        crate::config::Config::save_host_keys(&self.target, self.host_keys)
    }

    fn arguments(&self) -> [&str; 7] {
        [
            "machine",
            "add",
            &self.target,
            "--label",
            &self.label,
            "--remote-session",
            &self.session,
        ]
    }
}

// The relay's command line is typed into a terminal shell. Single-quote
// each word so a path never becomes shell syntax.
#[cfg(unix)]
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// The variables that choose where Herdr keeps its device catalog. Every CLI
/// run for a device gets this process's values, so it edits the same catalog.
const CATALOG_ROOTS: [&str; 3] = ["HERDR_CONFIG_PATH", "XDG_STATE_HOME", "XDG_CONFIG_HOME"];

/// The environment of every Herdr CLI run for a device: the user's `PATH`
/// and agent, not launchd's, since `machine add` and `ssh-copy-id` run the
/// user's own `ssh`, and the Tailscale CLI finds the Mac app's daemon with
/// `/usr/sbin/lsof`; and this window's catalog. Blocks the first time the
/// login environment is read: call it off the UI thread.
fn cli_environment(command: &mut Command) {
    crate::login_env::apply(command);
    for key in CATALOG_ROOTS {
        match std::env::var_os(key) {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
}

/// Run by `sh -c` before `machine add` when the user's key is to be
/// installed first: `$1` is the target, and the rest is the save command.
#[cfg(unix)]
const INSTALL_KEY_THEN: &str = "target=$1; shift; ssh-copy-id \"$target\" && exec \"$@\"";

/// What the setup terminal runs, as a child of `app` (this executable): the
/// session helper, then `machine add`, after `ssh-copy-id` for `install_key`.
/// Arguments stay separate words; no shell parses the target or label.
#[cfg(unix)]
fn setup_command(
    app: &std::ffi::OsStr,
    herdr: &std::ffi::OsStr,
    request: &Request,
    install_key: bool,
) -> Command {
    let mut command = herdr_client::new_command(app);
    command.arg(crate::setup_relay::SESSION);
    if install_key {
        command.args(["/bin/sh", "-c", INSTALL_KEY_THEN, "sh", request.target()]);
    }
    command.arg(herdr).args(request.arguments());
    command
}

/// The pane's end of a terminal setup that this app runs, while it waits.
#[cfg(unix)]
pub(super) type Relay = crate::setup_relay::Listening<Claim>;

/// Windows has no terminal setup, so there is never a relay.
#[cfg(windows)]
#[derive(Debug)]
pub(super) enum Relay {}

#[cfg(windows)]
impl Relay {
    pub(super) fn keep(&mut self, _: Claim) {
        match *self {}
    }
}

/// A setup that needs prompts: this app runs it on a terminal of its own and
/// a local workspace shows it (see `setup_relay` for why).
#[derive(Debug)]
pub(super) struct TerminalSetup {
    /// The line typed into the workspace shell: a short relay command, well
    /// under the 1024 bytes a shell in canonical mode accepts on macOS.
    pub(super) command: String,
    pub(super) relay: Relay,
}

/// Runs `machine add` without a terminal, for a host whose probe found a
/// compatible Herdr: the CLI starts a stopped server itself. Blocks on SSH, so
/// call it from the background executor. Stdin is closed, so any approval the
/// CLI would need fails instead of waiting for input nobody can see.
pub(super) fn save(request: &Request, claim: &Claim) -> Result<SaveOutcome> {
    request.remember_host_keys()?;
    let result = save_with(
        crate::daemon::executable(),
        request,
        claim,
        load_catalog,
        resolve,
    );
    finish_save(
        result,
        || request.probe(),
        || {
            verify_unsaved(request, claim)?;
            let host = SavedHost {
                id: uuid::Uuid::new_v4().simple().to_string(),
                label: request.label.clone(),
                target: request.target.clone(),
                session: request.session.clone(),
                enabled: true,
            };
            let saved = herdr_client::store_saved_host(false, &host)?;
            if saved.id != host.id {
                return Err(Error::Exists(saved.label).into());
            }
            Ok(())
        },
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveOutcome {
    // keep-sorted start
    ExistingServer,
    Ready,
    // keep-sorted end
}

impl SaveOutcome {
    /// The message the dialog shows once the device is saved, if any.
    pub(super) fn warning(self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::ExistingServer => Some("devices-setup-existing-server"),
        }
    }
}

fn finish_save(
    result: Result<()>,
    probe: impl FnOnce() -> Result<HostProbe>,
    store: impl FnOnce() -> Result<()>,
) -> Result<SaveOutcome> {
    match result {
        Ok(()) => Ok(SaveOutcome::Ready),
        Err(crate::Error::Devices(Error::ServerRestart)) => {
            // The CLI's provisioning requirements are stricter than this
            // client's endpoint protocol. Recheck after its failed attempt:
            // only a still-running, compatible host can use the local save.
            if probe()? != HostProbe::Running {
                return Err(Error::ServerRestart.into());
            }
            store()?;
            tracing::warn!(
                category = "device_setup",
                "Device saved with its existing remote server"
            );
            Ok(SaveOutcome::ExistingServer)
        }
        Err(error) => Err(error),
    }
}

/// The catalog is checked right before `machine add` and again after it. A
/// client outside this process can still save the same host in between,
/// because the CLI takes no lock; if so, the profile saved second is removed.
/// Every client applies that same rule, so exactly one profile survives even
/// when both notice the duplicate.
fn save_with(
    executable: impl AsRef<std::ffi::OsStr>,
    request: &Request,
    claim: &Claim,
    load: impl Fn() -> Result<Vec<SavedHost>>,
    resolve: impl Fn(&str) -> Option<Destination> + Copy,
) -> Result<()> {
    let executable = executable.as_ref();
    ensure_unsaved(request, claim, &load()?, resolve)?;
    let (status, stdout, stderr) = run_cli(executable, &request.arguments(), SAVE_TIMEOUT)?;
    if !status.success() {
        let detail = last_line(&stderr);
        // Without a terminal to confirm in, the CLI refuses rather than
        // restart a server whose panes would stop.
        if detail.contains(RESTART_REFUSAL) {
            return Err(Error::ServerRestart.into());
        }
        return Err(Error::Setup { status, detail }.into());
    }
    // Without the ID the CLI printed, there is no profile known to be ours.
    let Some(id) = saved_id(&stdout) else {
        tracing::warn!(
            category = "device_setup",
            "machine add printed no profile ID"
        );
        return Ok(());
    };
    tracing::info!(category = "device_setup", id, "machine add saved a profile");
    let hosts = load()?;
    match first_saved(request, claim, &hosts, resolve) {
        Some(first) if first.id != id => {
            tracing::warn!(
                category = "device_setup",
                id,
                first = first.id,
                "Another client saved this host first; removing ours"
            );
            remove_with(executable, &id)?;
            Err(Error::Exists(first.label.clone()).into())
        }
        _ => Ok(()),
    }
}

/// How `machine add` ends when the running server must be restarted first.
// i18n: data
const RESTART_REFUSAL: &str = "remote server is not ready for saved machines";

/// The profile ID from `Saved SSH machine <id>. Remote server is ready.`
fn saved_id(stdout: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stdout).lines().find_map(|line| {
        // The CLI's own output, in English.
        // i18n: data
        let id = line.strip_prefix("Saved SSH machine ")?.split('.').next()?;
        herdr_client::valid_profile_id(id).then(|| id.to_owned())
    })
}

/// Runs the CLI with closed stdin and a deadline, keeping bounded stdout and
/// stderr.
fn run_cli(
    executable: &std::ffi::OsStr,
    args: &[&str],
    timeout: Duration,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    run_bounded(executable, args, timeout, SAVE_STDERR_LIMIT)
}

/// Runs a program with closed stdin and a deadline, keeping at most `limit`
/// bytes of each of stdout and stderr. Both pipes are drained so a chatty
/// program never blocks on a full one. A missed deadline kills the program and
/// returns `Error::SetupTimeout`. Blocks, also on the first login-environment
/// lookup: call it from the background executor.
pub(super) fn run_bounded(
    executable: &std::ffi::OsStr,
    args: &[&str],
    timeout: Duration,
    limit: u64,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let started = Instant::now();
    let result = run_bounded_untraced(executable, args, timeout, limit);
    // Only the subcommand: later arguments name hosts and labels, and the
    // output can carry remote text, so neither is logged.
    let command = &args[..args.len().min(2)];
    let elapsed_ms = started.elapsed().as_millis() as u64;
    match &result {
        Ok((status, ..)) => tracing::info!(
            category = "device_cli",
            ?command,
            %status,
            elapsed_ms,
            "Device command finished"
        ),
        Err(_) => tracing::warn!(
            category = "device_cli",
            ?command,
            elapsed_ms,
            "Device command failed to run"
        ),
    }
    result
}

fn run_bounded_untraced(
    executable: &std::ffi::OsStr,
    args: &[&str],
    timeout: Duration,
    limit: u64,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let mut command = herdr_client::new_command(executable);
    cli_environment(&mut command);
    let mut child = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::Builder::new()
            .name("herdr-device-cli".into())
            .spawn(move || {
                let mut output = Vec::new();
                if let Some(mut pipe) = pipe {
                    let _ = (&mut pipe).take(limit).read_to_end(&mut output);
                    let _ = std::io::copy(&mut pipe, &mut std::io::sink());
                }
                output
            })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    )?;
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    )?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::SetupTimeout.into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok((status, stdout, stderr))
}

/// The CLI ends with its most specific error; earlier lines are progress.
pub(super) fn last_line(output: &[u8]) -> String {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(300)
        .collect()
}

#[cfg(all(test, unix))]
impl TerminalSetup {
    pub(super) fn fixture(command: &str) -> Self {
        Self {
            command: command.into(),
            relay: Relay::fixture(),
        }
    }
}

/// Start the setup for a local workspace: this app runs `machine add`, after
/// `ssh-copy-id` for `install_key`, once the workspace's relay connects. The
/// relay takes the host's claim and releases it when the setup ends. Blocks on the login environment and
/// the file system: call it off the UI thread.
#[cfg(unix)]
pub(super) fn terminal_setup(request: &Request, install_key: bool) -> Result<TerminalSetup> {
    request.remember_host_keys()?;
    let app = std::env::current_exe()?;
    let dir = crate::preferences::state_dir().ok_or(Error::NoStateDirectory)?;
    let mut command = setup_command(
        app.as_os_str(),
        crate::daemon::executable().as_os_str(),
        request,
        install_key,
    );
    cli_environment(&mut command);
    let relay = crate::setup_relay::listen(&dir, command, SETUP_LIMIT)?;
    let command = relay_line(&app, relay.path())?;
    Ok(TerminalSetup { command, relay })
}

/// The line the workspace's shell runs: this executable relaying `socket`.
/// It carries no environment, so its length depends only on the two paths.
#[cfg(unix)]
fn relay_line(app: &std::path::Path, socket: &std::path::Path) -> Result<String> {
    let word = |path: &std::path::Path| path.to_str().map(quote).ok_or(Error::NonUtf8SetupPath);
    Ok(format!(
        "{} {} {}",
        word(app)?,
        quote(crate::setup_relay::ATTACH),
        word(socket)?
    ))
}

/// Windows has no relay for a terminal setup yet; the dialog asks the user to
/// run `herdr machine add` in a terminal instead and never calls this.
#[cfg(windows)]
pub(super) fn terminal_setup(_: &Request, _: bool) -> Result<TerminalSetup> {
    Err(Error::TerminalSetupUnavailable.into())
}

#[cfg(test)]
mod tests;
