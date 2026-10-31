//! The blocking jobs behind Railway's Settings tab and its device picker row:
//! make a named key, reach `railway.new` headlessly with the chosen key, check
//! for (and on approval install) Herdr, and save the device with the key's path.
//!
//! Making the local key is a separate, earlier step and creates nothing at
//! Railway. Setup then probes `railway.new` headlessly with the key: Railway
//! refuses that (exit 13) for a key with no VM, and only then does setup run
//! the one connection that creates it, on a pseudo-terminal (`first_use`).
//! Either way it ends by reaching the VM headlessly, so a reconnect later
//! behaves exactly like setup's last step.

use super::{Error, Result, first_use, keys, refusal, ssh};
use crate::cloud::{self, CloudProvider, SavedDevice, install};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// The session every Railway device attaches to.
const SESSION: &str = "default";
const NAME_LIMIT: usize = 40;

/// What setting up a Railway device needs.
pub struct AddRequest {
    /// The private key to present, by absolute path.
    pub key: PathBuf,
    /// What a new device is called, here and while setup runs.
    pub name: String,
    /// The user approved running Herdr's installer if the VM lacks it.
    pub install: bool,
}

/// A default name for a new device.
pub fn random_name() -> String {
    cloud::names::random("railway", NAME_LIMIT)
}

/// Railway's own exit status for a headless connection by a key with no VM.
const REFUSED: i32 = 13;

/// The user's home directory, where `~/.ssh` is looked for.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Make a key named `name` and return its path. Creates only a local key
/// pair; nothing reaches Railway.
pub fn generate_key(name: &str, cancelled: &dyn Fn() -> bool) -> Result<PathBuf> {
    let dir = ssh::state_dir()?;
    keys::generate(
        &keys::key_directory(home().as_deref(), &dir),
        name,
        &cancelled,
    )
}

/// The whole job for `cloud::Jobs`.
pub fn add_device(
    request: AddRequest,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    let dir = ssh::state_dir()?;
    Ok(add_device_in(
        &dir,
        request,
        cancelled,
        report,
        cloud::load,
        cloud::save,
    )?)
}

fn add_device_in(
    dir: &Path,
    request: AddRequest,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
    load: impl FnOnce() -> cloud::Result<Vec<SavedDevice>>,
    save: impl FnOnce(SavedDevice) -> cloud::Result<()>,
) -> Result<SavedDevice> {
    let AddRequest { key, name, install } = request;
    // Before a VM could be created: a key that cannot be used, or a device
    // list that cannot be read, must not cost one of Railway's daily VMs.
    let key = keys::existing(&key)?;
    let id = keys::fingerprint(&key, &cancelled)?;
    let known = load()?
        .into_iter()
        .find(|saved| saved.provider == CloudProvider::Railway && saved.id == id);
    let known_hosts = ssh::known_hosts(dir);
    let headless = || ssh::command(&key, &known_hosts, false);
    report(cloud::Step::Connecting);
    if !vm_exists(headless()?, cancelled)? {
        report(cloud::Step::Creating);
        create_vm(ssh::command(&key, &known_hosts, true)?, cancelled)?;
        report(cloud::Step::Connecting);
        if !vm_exists(headless()?, cancelled)? {
            return Err(Error::CreateRefused(
                "the session ended without creating one".into(),
            ));
        }
    }
    ensure_herdr(headless, install, cancelled, report)?;
    // Setting a device up again keeps its name.
    let device = SavedDevice {
        provider: CloudProvider::Railway,
        id,
        label: known
            .as_ref()
            .map_or(name.clone(), |saved| saved.label.clone()),
        account: key.to_str().ok_or(Error::KeyPath)?.to_owned(),
        machine: known.map_or(name, |saved| saved.machine),
        session: SESSION.to_owned(),
        enabled: true,
    };
    save(device.clone())?;
    Ok(device)
}

/// Whether `railway.new` has a VM for the key, by a bare headless `ssh -T`
/// with no remote command. This never creates a VM and never retries. A
/// refusal means "no VM" only when Railway says a terminal is needed; any
/// other refusal, such as the daily limit, is final.
fn vm_exists(ssh: Command, cancelled: &dyn Fn() -> bool) -> Result<bool> {
    let finished = install::reach(ssh, &cancelled)?;
    match finished.code {
        Some(0) => Ok(true),
        Some(REFUSED) => match refusal::refusal(&finished.output) {
            refusal::Refusal::NeedsTerminal => Ok(false),
            refusal::Refusal::Other => Err(Error::CreateRefused(failure_text(&finished.output))),
        },
        _ => Err(Error::Unreachable(failure_text(&finished.output))),
    }
}

fn failure_text(output: &str) -> String {
    if output.is_empty() {
        "the ssh command failed".into()
    } else {
        refusal::hide_links(output)
    }
}

/// The one connection that creates the VM, on a pseudo-terminal, for a key a
/// headless probe just found has none. Nothing is read from it but Railway's
/// own words if it refuses (its daily limit, for one): the probe that follows
/// says whether the VM exists.
fn create_vm(mut session: Command, cancelled: &dyn Fn() -> bool) -> Result<()> {
    session.arg(install::remote("true"));
    let finished = first_use::create_vm(session, cancelled)?;
    match finished.code {
        Some(0) => Ok(()),
        Some(REFUSED) => Err(Error::CreateRefused(failure_text(&finished.output))),
        _ => Err(Error::Unreachable(failure_text(&finished.output))),
    }
}

/// A failure's output tail can hold a link from Railway; it must not reach a
/// toast.
fn hidden<T>(result: cloud::Result<T>) -> Result<T> {
    result.map_err(|error| match error {
        cloud::Error::Install(install::Failure::Output(text)) => {
            cloud::Error::Install(install::Failure::Output(refusal::hide_links(&text))).into()
        }
        error => error.into(),
    })
}

/// Make sure the VM has a Herdr the bridge can run, installing it only when
/// the user approved. `session` builds a fresh `ssh` command for each remote
/// command; setup is the only caller, so no reconnect ever installs.
fn ensure_herdr(
    session: impl Fn() -> Result<Command>,
    install: bool,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> Result<()> {
    if hidden(install::installed(session()?, &cancelled))? {
        return Ok(());
    }
    if !install {
        return Err(cloud::Error::Install(install::Failure::NotInstalled {
            machine: None,
            retry: install::Retry::Allow,
        })
        .into());
    }
    report(cloud::Step::InstallingHerdr);
    hidden(install::install(session()?, &cancelled))?;
    // The installer's own success is not proof the bridge will find it.
    if !hidden(install::installed(session()?, &cancelled))? {
        return Err(cloud::Error::Install(install::Failure::Missing).into());
    }
    Ok(())
}

/// One saved device, for the Settings tab: its account is the path of its key.
#[derive(Debug)]
pub struct Device {
    pub saved: SavedDevice,
    pub key_present: bool,
}

impl Device {
    pub fn key_path(&self) -> &Path {
        Path::new(&self.saved.account)
    }
}

/// What the Settings tab shows.
#[derive(Debug)]
pub struct Overview {
    pub devices: Vec<Device>,
    /// Where a newly generated key will be created.
    pub new_key_directory: keys::KeyDirectory,
}

pub fn overview() -> Result<Overview> {
    let dir = ssh::state_dir()?;
    Ok(overview_in(&dir, home().as_deref(), cloud::load()?))
}

fn overview_in(dir: &Path, home: Option<&Path>, saved: Vec<SavedDevice>) -> Overview {
    Overview {
        devices: saved
            .into_iter()
            .filter(|saved| saved.provider == CloudProvider::Railway)
            .map(|saved| Device {
                key_present: Path::new(&saved.account).is_file(),
                saved,
            })
            .collect(),
        new_key_directory: keys::key_directory(home, dir),
    }
}

/// Forget one saved device. The VM is left alone at Railway (it expires on its
/// own) and no key file is ever deleted.
pub fn forget_device(id: &str) -> Result<()> {
    Ok(cloud::remove(CloudProvider::Railway, id)?)
}

/// Expand a leading `~/` in a typed path.
pub fn expand(text: &str) -> PathBuf {
    match (text.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest),
        _ => PathBuf::from(text),
    }
}

// Tests drive `ssh`, `ssh-keygen`, and `/bin/sh`; cloud devices are Unix-only.
#[cfg(all(test, unix))]
mod tests;
// The terminal session that creates a VM, against a stand-in for Railway.
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod create_tests;
