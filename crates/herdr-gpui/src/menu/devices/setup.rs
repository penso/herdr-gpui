//! Delegate provisioning to the installed Herdr CLI. A host that already runs
//! a compatible Herdr is saved without a terminal; anything that needs SSH or
//! installation prompts runs in a local workspace, so no remote installer
//! policy is duplicated here.
use crate::{Error, Result};
use herdr_client::{Destination, HostProbe, SavedHost};
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
/// How long a claim keeps this process from adding its host again after a
/// terminal setup starts: that setup can take this long to finish its prompts.
const CLAIM_TTL: Duration = Duration::from_secs(15 * 60);

/// Hosts that some window of this process is adding. Herdr's `machine add`
/// neither locks the catalog nor refuses duplicates, so this is what keeps two
/// windows, or two clicks, from adding the same host at once.
static CLAIMS: Mutex<Vec<(Destination, String, Instant)>> = Mutex::new(Vec::new());

/// This process is adding one host and session. Dropping it releases the
/// host, unless `hold` kept it for a terminal setup that is still running.
#[derive(Debug)]
pub(super) struct Claim {
    destination: Destination,
    session: String,
    held: bool,
}

impl Claim {
    fn acquire(destination: Destination, session: &str, now: Instant) -> Result<Self> {
        let mut claims = CLAIMS.lock().unwrap_or_else(|error| error.into_inner());
        claims.retain(|(.., since)| now.duration_since(*since) < CLAIM_TTL);
        if claims.iter().any(|(claimed, claimed_session, _)| {
            *claimed == destination && claimed_session == session
        }) {
            return Err(Error::DeviceAdding);
        }
        claims.push((destination.clone(), session.to_owned(), now));
        Ok(Self {
            destination,
            session: session.to_owned(),
            held: false,
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
        Self::acquire(destination, "default", Instant::now()).unwrap_or(Self {
            destination: Destination {
                user: "test".into(),
                host: host.into(),
                port: 22,
            },
            session: "default".into(),
            held: true,
        })
    }

    /// Keep the host claimed until `CLAIM_TTL`, because the terminal that runs
    /// `machine add` outlives this dialog and cannot report when it finishes.
    pub(super) fn hold(mut self) {
        self.held = true;
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if self.held {
            return;
        }
        let mut claims = CLAIMS.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(index) = claims.iter().position(|(claimed, session, _)| {
            *claimed == self.destination && *session == self.session
        }) {
            claims.remove(index);
        }
    }
}

/// Resolve the request's host, claim it for this process, then check the
/// catalog on disk. Claiming first means a concurrent add in this process sees
/// the claim even before the catalog changes. Blocks on `ssh -G` and the
/// catalog file: call it from the background executor.
pub(super) fn claim(request: &Request) -> Result<Claim> {
    let destination = herdr_client::resolve_destination(&request.target)?;
    let claim = Claim::acquire(destination, &request.session, Instant::now())?;
    ensure_unsaved(request, &claim, &load_catalog()?, resolve)?;
    Ok(claim)
}

/// Claim a saved host while it is removed, so an add of the same host from
/// this process cannot run at the same time. Blocks on `ssh -G`.
pub(super) fn claim_saved(target: &str, session: &str) -> Result<Claim> {
    let destination = herdr_client::resolve_destination(target)?;
    Claim::acquire(destination, session, Instant::now())
}

/// Forget a saved device with `herdr machine remove`. The CLI only edits the
/// local catalog; it never connects, so the host's own Herdr keeps running.
/// Blocks on a process: call it from the background executor.
pub(super) fn remove(id: &str) -> Result<()> {
    remove_with(crate::daemon::executable().as_os_str(), id)
}

fn remove_with(executable: &std::ffi::OsStr, id: &str) -> Result<()> {
    if !herdr_client::valid_profile_id(id) {
        return Err(Error::DeviceSetupInput("This device has no saved profile."));
    }
    let (status, _, stderr) = run_cli(executable, &["machine", "remove", id], REMOVE_TIMEOUT)?;
    if status.success() {
        return Ok(());
    }
    Err(Error::DeviceRemove {
        status,
        detail: last_line(&stderr),
    })
}

/// Rename a saved device with `herdr machine rename`, which edits only the
/// local catalog. Blocks on a process: call it from the background executor.
pub(super) fn rename(id: &str, label: &str) -> Result<()> {
    rename_with(crate::daemon::executable().as_os_str(), id, label)
}

fn rename_with(executable: &std::ffi::OsStr, id: &str, label: &str) -> Result<()> {
    if !herdr_client::valid_profile_id(id) {
        return Err(Error::DeviceSetupInput("This device has no saved profile."));
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
    Err(Error::DeviceRename {
        status,
        detail: last_line(&stderr),
    })
}

/// Check the catalog on disk again, right before a step that saves.
pub(super) fn verify_unsaved(request: &Request, claim: &Claim) -> Result<()> {
    ensure_unsaved(request, claim, &load_catalog()?, resolve)
}

/// Device setup is refused for development catalogs, so this is the catalog
/// `machine add` writes: the terminal command restores the same state root.
fn load_catalog() -> Result<Vec<SavedHost>> {
    Ok(herdr_client::load_saved_hosts(false)?)
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
        Some(host) => Err(Error::DeviceExists(host.label.clone())),
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
        return Err(Error::DeviceSetupInput(
            "Device names are at most 128 bytes, without control characters.",
        ));
    }
    Ok(label.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Request {
    target: String,
    label: String,
    session: String,
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
            return Err(Error::DeviceSetupInput(
                "Enter an SSH target such as user@hostname or an SSH alias.",
            ));
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
            return Err(Error::DeviceSetupInput(
                "Session names use letters, digits, dots, underscores, or hyphens (at most 64 bytes), excluding '.' and '..'.",
            ));
        }
        Ok(Self {
            target: target.into(),
            label,
            session: session.into(),
        })
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
        Ok(herdr_client::probe_host(&self.target, &self.session)?)
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

// The command is typed into a terminal shell.
// Single-quote each argument so labels/SSH aliases never become shell syntax.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn shell_command(executable: &str, request: &Request, environment: &[(String, String)]) -> String {
    let mut args = vec!["env".to_owned()];
    // The daemon's shell does not inherit the GUI's environment. Clear
    // optional overrides before restoring this window's exact catalog roots.
    for name in ["HERDR_CONFIG_PATH", "XDG_STATE_HOME", "XDG_CONFIG_HOME"] {
        args.extend(["-u".into(), name.into()]);
    }
    args.extend(
        environment
            .iter()
            .map(|(key, value)| format!("{key}={value}")),
    );
    args.push(executable.into());
    args.extend(request.arguments().into_iter().map(str::to_owned));
    args.iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs `machine add` without a terminal, for a host whose probe found a
/// compatible Herdr: the CLI starts a stopped server itself. Blocks on SSH, so
/// call it from the background executor. Stdin is closed, so any approval the
/// CLI would need fails instead of waiting for input nobody can see.
pub(super) fn save(request: &Request, claim: &Claim) -> Result<()> {
    save_with(
        crate::daemon::executable(),
        request,
        claim,
        load_catalog,
        resolve,
    )
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
        return Err(Error::DeviceSetup {
            status,
            detail: last_line(&stderr),
        });
    }
    // Without the ID the CLI printed, there is no profile known to be ours.
    let Some(id) = saved_id(&stdout) else {
        return Ok(());
    };
    let hosts = load()?;
    match first_saved(request, claim, &hosts, resolve) {
        Some(first) if first.id != id => {
            remove_with(executable, &id)?;
            Err(Error::DeviceExists(first.label.clone()))
        }
        _ => Ok(()),
    }
}

/// The profile ID from `Saved SSH machine <id>. Remote server is ready.`
fn saved_id(stdout: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stdout).lines().find_map(|line| {
        let id = line.strip_prefix("Saved SSH machine ")?.split('.').next()?;
        herdr_client::valid_profile_id(id).then(|| id.to_owned())
    })
}

/// Runs the CLI with closed stdin and a deadline, keeping bounded stdout and
/// stderr. Both pipes are drained so a chatty CLI never blocks on a full one.
fn run_cli(
    executable: &std::ffi::OsStr,
    args: &[&str],
    timeout: Duration,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let mut child = Command::new(executable)
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
                    let _ = (&mut pipe).take(SAVE_STDERR_LIMIT).read_to_end(&mut output);
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
            return Err(Error::DeviceSetupTimeout);
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok((status, stdout, stderr))
}

/// The CLI ends with its most specific error; earlier lines are progress.
fn last_line(output: &[u8]) -> String {
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

/// The shell command a local workspace runs to set the host up interactively.
pub(super) fn terminal_command(request: &Request) -> Result<String> {
    if cfg!(windows) {
        return Err(Error::DeviceSetupInput(
            "Saved SSH devices are unavailable on Windows.",
        ));
    }
    let executable = crate::daemon::executable();
    let executable = executable.to_str().ok_or(Error::DeviceSetupInput(
        "The Herdr executable path must be UTF-8.",
    ))?;
    let mut environment = Vec::new();
    for key in [
        "HOME",
        "PATH",
        "HERDR_CONFIG_PATH",
        "XDG_STATE_HOME",
        "XDG_CONFIG_HOME",
    ] {
        if let Some(value) = std::env::var_os(key) {
            environment.push((
                key.into(),
                value
                    .into_string()
                    .map_err(|_| Error::DeviceSetupInput("The setup environment must be UTF-8."))?,
            ));
        }
    }
    Ok(shell_command(executable, request, &environment))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_setup_fields_before_launch() -> Result<()> {
        for target in ["", "-oProxyCommand=bad", "two hosts", "host\ncommand"] {
            assert!(matches!(
                Request::new(target, "Label", ""),
                Err(Error::DeviceSetupInput(_))
            ));
        }
        // An empty label is not an error: the target names the device.
        assert_eq!(Request::new("host", "", "")?.label(), "host");
        assert!(Request::new("host", "Label", "../session").is_err());
        assert!(Request::new("host", "Label", ".").is_err());
        assert!(Request::new("host", "Label", "dev.session").is_ok());
        assert!(Request::new("ssh://user:password@host", "Label", "").is_err());
        let request = Request::new(" user@host ", " My Device ", "")?;
        assert_eq!(
            request.arguments(),
            [
                "machine",
                "add",
                "user@host",
                "--label",
                "My Device",
                "--remote-session",
                "default"
            ]
        );
        Ok(())
    }

    fn destination(host: &str) -> Destination {
        Destination {
            user: "penso".into(),
            host: host.into(),
            port: 22,
        }
    }

    fn saved(id: char, label: &str, target: &str) -> SavedHost {
        SavedHost {
            id: id.to_string().repeat(32),
            label: label.into(),
            target: target.into(),
            session: "default".into(),
            enabled: true,
        }
    }

    #[test]
    fn a_claim_blocks_a_second_add_until_released_held_or_expired() -> Result<()> {
        let now = Instant::now();
        let first = Claim::acquire(destination("claim.test"), "default", now)?;
        assert!(matches!(
            Claim::acquire(destination("claim.test"), "default", now),
            Err(Error::DeviceAdding)
        ));
        // Another session on the same host is a different device.
        drop(Claim::acquire(destination("claim.test"), "work", now)?);
        drop(first);
        let held = Claim::acquire(destination("claim.test"), "default", now)?;
        held.hold();
        assert!(Claim::acquire(destination("claim.test"), "default", now).is_err());
        drop(Claim::acquire(
            destination("claim.test"),
            "default",
            now + CLAIM_TTL,
        )?);
        Ok(())
    }

    #[test]
    fn saved_hosts_match_by_resolved_destination_not_spelling() -> Result<()> {
        let request = Request::new("penso@10.0.0.9", "Box", "")?;
        let claim = Claim::acquire(destination("match.test"), "default", Instant::now())?;
        let resolve = |target: &str| {
            (target == "box-alias" || target == "penso@10.0.0.9").then(|| destination("match.test"))
        };
        let mut other_session = saved('b', "Work", "box-alias");
        other_session.session = "work".into();
        let hosts = [
            saved('a', "Else", "elsewhere"),
            other_session,
            saved('c', "Box", "box-alias"),
        ];
        assert_eq!(
            first_saved(&request, &claim, &hosts, resolve).map(|host| host.label.as_str()),
            Some("Box")
        );
        assert!(ensure_unsaved(&request, &claim, &hosts[..2], resolve).is_ok());
        Ok(())
    }

    #[test]
    fn saved_ids_come_only_from_the_cli_confirmation_line() {
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            saved_id(
                format!("progress\nSaved SSH machine {id}. Remote server is ready.\n").as_bytes()
            ),
            Some(id.into())
        );
        assert_eq!(
            saved_id(b"Saved SSH machine ../x. Remote server is ready.\n"),
            None
        );
        assert_eq!(saved_id(b""), None);
    }

    /// A fake `herdr` that prints the confirmation `machine add` prints, logs
    /// removals, and fails the add of `host` the way a needed approval does.
    #[cfg(unix)]
    fn fake_cli(name: &str) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("herdr-device-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let binary = root.join("herdr");
        let log = root.join("log");
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\ncase \"$1 $2\" in\n\
                 'machine add') [ \"$3\" = host ] && {{ read -r answer; echo 'error: approval required' >&2; exit 3; }}\n\
                   echo add >> '{log}'; echo \"Saved SSH machine {id}. Remote server is ready.\";;\n\
                 'machine remove') echo \"remove $3\" >> '{log}';;\n\
                 esac\n",
                log = log.display(),
                id = "a".repeat(32),
            ),
        )?;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))?;
        Ok((binary, log))
    }

    #[cfg(unix)]
    #[test]
    fn save_reports_the_cli_failure_without_waiting_for_input() -> Result<()> {
        let (binary, log) = fake_cli("failure")?;
        let request = Request::new("host", "Label", "")?;
        let claim = Claim::acquire(destination("failure.test"), "default", Instant::now())?;
        let error = save_with(&binary, &request, &claim, || Ok(Vec::new()), |_| None).err();
        std::fs::remove_dir_all(binary.parent().unwrap_or(&binary))?;
        assert!(!log.exists());
        match error {
            Some(Error::DeviceSetup { status, detail }) => {
                assert_eq!(status.code(), Some(3));
                assert_eq!(detail, "error: approval required");
            }
            other => panic!("unexpected result: {other:?}"),
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_host_saved_by_another_client_meanwhile_keeps_only_the_first_profile() -> Result<()> {
        let (binary, log) = fake_cli("race")?;
        let request = Request::new("penso@box", "Mine", "")?;
        let claim = Claim::acquire(destination("race.test"), "default", Instant::now())?;
        let resolve = |_: &str| Some(destination("race.test"));
        let ours = saved('a', "Mine", "penso@box");
        let theirs = saved('b', "Theirs", "box-alias");
        let run = |after: Vec<SavedHost>| {
            let loads = std::cell::Cell::new(0);
            let result = save_with(
                &binary,
                &request,
                &claim,
                || {
                    loads.set(loads.get() + 1);
                    Ok(if loads.get() == 1 {
                        Vec::new()
                    } else {
                        after.clone()
                    })
                },
                resolve,
            );
            let calls = std::fs::read_to_string(&log).unwrap_or_default();
            let _ = std::fs::remove_file(&log);
            (result, calls)
        };

        // Nobody else saved it: ours stays.
        let (result, calls) = run(vec![ours.clone()]);
        assert!(result.is_ok());
        assert_eq!(calls, "add\n");
        // Another client saved it first: ours is removed, theirs stays.
        let (result, calls) = run(vec![theirs.clone(), ours.clone()]);
        assert!(matches!(result, Err(Error::DeviceExists(label)) if label == "Theirs"));
        assert_eq!(calls, format!("add\nremove {}\n", "a".repeat(32)));
        // Ours came first: the other client removes its own, not us.
        let (result, calls) = run(vec![ours, theirs.clone()]);
        assert!(result.is_ok());
        assert_eq!(calls, "add\n");

        // Already saved before we start: `machine add` never runs.
        let before = save_with(
            &binary,
            &request,
            &claim,
            || Ok(vec![theirs.clone()]),
            resolve,
        );
        assert!(matches!(before, Err(Error::DeviceExists(_))));
        assert!(!log.exists());
        std::fs::remove_dir_all(binary.parent().unwrap_or(&binary))?;
        Ok(())
    }

    #[test]
    fn an_empty_label_names_the_device_after_its_target() -> Result<()> {
        assert_eq!(
            Request::new("penso@10.0.0.9", "  ", "")?.label(),
            "penso@10.0.0.9"
        );
        assert_eq!(device_label("", "ssh://penso@box:22")?, "penso@box:22");
        assert_eq!(device_label(" Work box ", "box")?, "Work box");
        // A long target is cut to Herdr's limit on a character boundary.
        let long = format!("u@{}", "é".repeat(100));
        let label = device_label("", &long)?;
        assert!(label.len() <= LABEL_LIMIT && long.starts_with(&label));
        assert!(device_label("bad\u{7}", "box").is_err());
        assert!(device_label(&"x".repeat(129), "box").is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rename_passes_the_label_as_one_value_even_when_it_looks_like_a_flag() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("herdr-device-rename-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let binary = root.join("herdr");
        let log = root.join("log");
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\nprintf '%s|' \"$@\" > '{}'\n[ \"$3\" = missing ] && {{ echo 'machine profile missing was not found' >&2; exit 1; }}\nexit 0\n",
                log.display()
            ),
        )?;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))?;
        let id = "0123456789abcdef0123456789abcdef";
        rename_with(binary.as_os_str(), id, "--work = box")?;
        assert_eq!(
            std::fs::read_to_string(&log)?,
            format!("machine|rename|{id}|--label=--work = box|")
        );
        assert!(matches!(
            rename_with(binary.as_os_str(), "../x", "x"),
            Err(Error::DeviceSetupInput(_))
        ));
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn same_host_means_same_ssh_target_and_session() -> Result<()> {
        // An empty session is the default one, as `machine add` saves it.
        let request = Request::new(" penso@box ", "Box", "")?;
        assert!(request.same_host("penso@box", "default"));
        assert!(!request.same_host("penso@box", "work"));
        assert!(!request.same_host("other@box", "default"));
        Ok(())
    }

    #[test]
    fn save_failures_keep_only_the_final_diagnostic_line() {
        assert_eq!(
            last_line(b"connecting\nerror: remote server is not ready\n\n"),
            "error: remote server is not ready"
        );
        assert_eq!(last_line(b"\x1b[31mbad\x1b[0m"), "[31mbad[0m");
        assert_eq!(last_line(&[b'x'; 1000]).len(), 300);
        assert_eq!(last_line(b""), "");
    }

    #[test]
    fn shell_arguments_and_catalog_roots_are_quoted() -> Result<()> {
        let request = Request::new("host", "Alice's $(printf INJECTED); device", "work")?;
        let command = shell_command(
            "/a path/herdr",
            &request,
            &[("XDG_STATE_HOME".into(), "/state user's".into())],
        );
        assert!(command.contains("'XDG_STATE_HOME=/state user'\\''s'"));
        assert!(command.contains("'/a path/herdr' 'machine' 'add' 'host'"));
        assert!(command.contains("'Alice'\\''s $(printf INJECTED); device'"));
        assert!(command.contains("'-u' 'HERDR_CONFIG_PATH'"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn terminal_shell_preserves_exact_arguments_without_expansion() -> Result<()> {
        let request = Request::new("user@host", "Alice's $(printf INJECTED); device", "work")?;
        let command = shell_command(
            "/a path/herdr",
            &request,
            &[("XDG_STATE_HOME".into(), "/state user's".into())],
        );
        let output = Command::new("/bin/sh")
            .args(["-c", &format!("set -- {command}; printf '%s\\0' \"$@\"")])
            .output()?;
        assert!(output.status.success());
        let args: Vec<_> = output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|value| !value.is_empty())
            .collect();
        assert_eq!(
            &args[args.len() - 7..],
            request.arguments().map(str::as_bytes).as_slice()
        );
        assert!(args.contains(&b"XDG_STATE_HOME=/state user's".as_slice()));
        assert!(args.contains(&b"/a path/herdr".as_slice()));
        Ok(())
    }
}
