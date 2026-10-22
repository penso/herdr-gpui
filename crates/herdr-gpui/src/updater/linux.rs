//! Native Linux packages are installed by their owning package manager. All
//! queries, downloads, authentication prompts and process waits run on the worker.
use super::{
    error::{Result, UpdateError as Error},
    install,
    release::{self, Format},
};
use std::{
    env, fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use tempfile::TempDir;

mod helper;
mod restart;
pub(super) use restart::{RestartGuard, relaunch};

const SYSTEM_EXECUTABLE: &str = "/usr/bin/herdr-gpui";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Package {
    pub(super) format: Format,
    executable: PathBuf,
}

pub(super) struct Prepared {
    stage: TempDir,
    package: Package,
    offer: release::Offer,
}

fn query(format: Format, executable: &Path) -> Command {
    let mut command = match format {
        Format::Deb => {
            let mut command = herdr_client::new_command("/usr/bin/dpkg-query");
            command.arg("--search");
            command
        }
        Format::Rpm => {
            let mut command = herdr_client::new_command("/usr/bin/rpm");
            command.args(["-qf", "--queryformat", "%{NAME}\n"]);
            command
        }
        Format::Arch => {
            let mut command = herdr_client::new_command("/usr/bin/pacman");
            command.arg("-Qoq");
            command
        }
        Format::Archive | Format::Installer => unreachable!(),
    };
    command.arg(executable);
    command
}

fn owns(format: Format, executable: &Path, output: &str) -> bool {
    if format != Format::Deb {
        return output.trim() == "herdr-gpui";
    }
    // dpkg may qualify the owner with its architecture. Match the complete
    // path as well: diversions and unrelated matches do not prove ownership.
    let Some((owner, path)) = output.trim().rsplit_once(": ") else {
        return false;
    };
    owner.split(':').next() == Some("herdr-gpui") && Path::new(path) == executable
}

fn locate(
    executable: &Path,
    mut run: impl FnMut(&mut Command) -> Result<String>,
) -> Result<Package> {
    let mut found = None;
    for format in [Format::Deb, Format::Rpm, Format::Arch] {
        match run(&mut query(format, executable)) {
            Ok(output) if owns(format, executable, &output) => {
                if found.is_some() {
                    return Err(Error::UnknownPackage);
                }
                found = Some(Package {
                    format,
                    executable: executable.into(),
                });
            }
            Ok(_) | Err(Error::ValidationFailed(_)) => (),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    found.ok_or(Error::UnknownPackage)
}

pub(super) fn detect(cancel: &AtomicBool) -> Result<Option<Package>> {
    if ["APPIMAGE", "SNAP", "FLATPAK_ID"]
        .iter()
        .any(|key| env::var_os(key).is_some())
    {
        return Err(Error::PackageManaged);
    }
    let executable = env::current_exe()?;
    if executable.starts_with("/nix/store") {
        return Err(Error::PackageManaged);
    }
    if !executable.starts_with("/usr") || executable.starts_with("/usr/local") {
        return Ok(None);
    }
    locate(&executable, |command| install::output(command, cancel)).map(Some)
}

pub(super) fn prepare(
    package: Package,
    offer: &release::Offer,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<Prepared> {
    if detect(cancel)?.as_ref() != Some(&package) {
        return Err(Error::PackageChanged);
    }
    if offer.manifest.schema != 2
        || offer.asset.name
            != package
                .format
                .name(&offer.manifest.version, &offer.asset.target)?
    {
        return Err(Error::OfferAsset);
    }
    let stage = tempfile::Builder::new()
        .prefix("herdr-package-")
        .tempdir()?;
    fs::set_permissions(stage.path(), fs::Permissions::from_mode(0o700))?;
    release::download(
        offer,
        &stage.path().join(&offer.asset.name),
        cancel,
        progress,
    )?;
    Ok(Prepared {
        stage,
        package,
        offer: offer.clone(),
    })
}

/// Only the privileged helper calls this, with a protected, verified copy.
fn install_command(format: Format, path: &Path) -> Command {
    let mut command = match format {
        Format::Deb => {
            let mut command = herdr_client::new_command("/usr/bin/apt-get");
            command.args(["install", "--yes", "--no-remove", "--"]);
            command
        }
        Format::Rpm => {
            // dnf5 rejects "--"; the path is always absolute, so it cannot
            // be parsed as an option.
            let mut command = herdr_client::new_command("/usr/bin/dnf");
            command.args(["install", "--assumeyes"]);
            command
        }
        Format::Arch => {
            let mut command = herdr_client::new_command("/usr/bin/pacman");
            command.args(["-U", "--noconfirm", "--"]);
            command
        }
        Format::Archive | Format::Installer => unreachable!(),
    };
    command
        .arg(path)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C")
        .env("DEBIAN_FRONTEND", "noninteractive")
        .current_dir("/")
        .stdin(Stdio::null());
    command
}

/// A transaction is never killed or cancelled, including when the GUI closes.
/// Fixed-size reads and a bounded channel keep arbitrary package output bounded.
fn transact(mut command: Command, mut progress: impl FnMut(String)) -> Result<()> {
    let program = PathBuf::from(command.get_program());
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::PackageCommand { program, source })?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        // Do not kill an installer even on this unreachable pipe setup failure.
        let _ = child.wait();
        return Err(Error::MissingValidationPipes);
    };
    let (sender, receiver) = mpsc::sync_channel(8);
    for mut pipe in [Box::new(stdout) as Box<dyn Read + Send>, Box::new(stderr)] {
        let sender = sender.clone();
        thread::spawn(move || {
            let mut buffer = [0; 1024];
            while let Ok(count) = pipe.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                // Continue draining if the receiver has gone away.
                let _ = sender.send(buffer[..count].to_vec());
            }
        });
    }
    drop(sender);
    let mut detail = String::new();
    let mut exited = None;
    loop {
        if exited.is_none() {
            exited = child
                .try_wait()?
                .map(|status| (status, std::time::Instant::now()));
        }
        if let Some((status, at)) = exited
            && at.elapsed() >= Duration::from_secs(2)
        {
            return transaction_result(status, detail);
        }
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(bytes) => {
                let text: String = String::from_utf8_lossy(&bytes)
                    .chars()
                    .filter(|c| !c.is_control() || *c == '\n')
                    .collect();
                if let Some(line) = text.lines().rev().find(|line| !line.trim().is_empty()) {
                    detail = line.trim().chars().take(240).collect();
                    progress(detail.clone());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => (),
        }
    }
    transaction_result(child.wait()?, detail)
}

fn transaction_result(status: std::process::ExitStatus, detail: String) -> Result<()> {
    if status.success() {
        return Ok(());
    }
    if matches!(status.code(), Some(126 | 127)) {
        return Err(Error::PackageAuthorization);
    }
    Err(Error::PackageFailed { status, detail })
}

fn installed_version(format: Format, cancel: &AtomicBool) -> Result<String> {
    let mut command = match format {
        Format::Deb => {
            let mut command = herdr_client::new_command("/usr/bin/dpkg-query");
            command.args(["-W", "-f=${Version}", "herdr-gpui"]);
            command
        }
        Format::Rpm => {
            let mut command = herdr_client::new_command("/usr/bin/rpm");
            command.args(["-q", "--queryformat", "%{VERSION}-%{RELEASE}", "herdr-gpui"]);
            command
        }
        Format::Arch => {
            let mut command = herdr_client::new_command("/usr/bin/pacman");
            command.args(["-Q", "herdr-gpui"]);
            command
        }
        Format::Archive | Format::Installer => unreachable!(),
    };
    let output = install::output(&mut command, cancel)?;
    Ok(output
        .trim()
        .strip_prefix("herdr-gpui ")
        .unwrap_or(output.trim())
        .to_owned())
}

pub(super) fn install(
    mut prepared: Prepared,
    cancel: &AtomicBool,
    progress: impl FnMut(String),
) -> Result<PathBuf> {
    if cancel.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    if detect(cancel)?.as_ref() != Some(&prepared.package) {
        return Err(Error::PackageChanged);
    }
    helper::write_request(&prepared)?;
    // A detached, non-elevated owner keeps draining the installer's pipes and
    // retains the package even if this GUI exits during authorization/install.
    // Once spawned, only that helper removes its staging directory.
    prepared.stage.disable_cleanup(true);
    let mut command = herdr_client::new_command(&prepared.package.executable);
    command
        .arg(helper::FLAG)
        .arg(prepared.stage.path())
        .stdin(Stdio::null());
    if let Err(error) = transact(command, progress) {
        if matches!(error, Error::PackageCommand { .. }) {
            prepared.stage.disable_cleanup(false);
        }
        return Err(error);
    }
    Ok(prepared.package.executable)
}

fn apply(
    package: &Package,
    offer: &release::Offer,
    stage: &Path,
    key: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(String),
) -> Result<()> {
    if locate(&package.executable, |command| {
        install::output(command, cancel)
    })? != *package
    {
        return Err(Error::PackageChanged);
    }
    let manifest = release::verify_manifest(
        &offer.manifest_bytes,
        &offer.signature,
        key,
        &offer.manifest.version,
    )?;
    if manifest != offer.manifest || !manifest.assets.contains(&offer.asset) {
        return Err(Error::UnauthenticatedOffer);
    }
    let path = stage.join(&offer.asset.name);
    release::verify_archive(&path, &offer.asset, cancel)?;
    if !Path::new("/usr/bin/pkexec").is_file() {
        return Err(Error::MissingPolicyKit);
    }
    progress("Approve the system authentication prompt to install the package...".into());
    transact(helper::elevated_command(stage)?, &mut progress)?;
    // Ignore cancellation after the transaction starts, including GUI teardown.
    let uncancelled = AtomicBool::new(false);
    let version = installed_version(package.format, &uncancelled)?;
    if version != format!("{}-1", manifest.version) {
        return Err(Error::PackageVersion);
    }
    Ok(())
}

pub(super) fn run_helper(args: &[std::ffi::OsString]) -> Option<std::process::ExitCode> {
    restart::run_helper(args).or_else(|| helper::run_helper(args))
}

#[cfg(test)]
mod tests;
