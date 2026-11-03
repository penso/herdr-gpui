//! The blocking jobs behind the OpenSandbox tab: make a sandbox, install
//! Herdr in it on approval, wait for the server its entrypoint keeps running,
//! and save the device; read what the tab shows; and delete a sandbox.

use super::{Result, Settings, api::Api, connect::relay_command, store};
use crate::{
    cloud::{self, CloudProvider, SavedDevice, Spec, install},
    credentials::Store,
};
use secrecy::SecretString;
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// The session every sandbox device attaches to.
const SESSION: &str = "default";
const NAME_LIMIT: usize = 40;
/// How long the entrypoint may take to start Herdr's server once installed.
const SERVER_TIMEOUT: Duration = Duration::from_secs(30);
const SERVER_UP: &str = r#"PATH="$HOME/.local/bin:$PATH"; herdr status server 2>/dev/null | grep -q '^status: running'"#;

/// What adding a sandbox needs.
pub struct AddRequest {
    pub settings: Settings,
    pub name: String,
    /// The user approved running Herdr's installer in the sandbox.
    pub install: bool,
}

pub fn random_name() -> String {
    cloud::names::random("herdr", NAME_LIMIT)
}

/// The whole job for `cloud::Jobs`: make the sandbox, install Herdr if
/// approved, wait for its server, and save the device. A failure after the
/// sandbox exists deletes it again, so nothing runs unlisted.
pub fn add_device(
    request: AddRequest,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    let AddRequest {
        settings,
        name,
        install,
    } = request;
    let key = store::key(&settings)?;
    let api = Api {
        settings: &settings,
        key: &key,
    };
    report(cloud::Step::Creating);
    let sandbox = api.create(&name)?;
    let ready = prepare(&api, &settings, &sandbox.id, install, cancelled, report).and_then(|()| {
        let device = SavedDevice {
            provider: CloudProvider::OpenSandbox,
            id: sandbox.id.clone(),
            label: name.clone(),
            account: settings.base.clone(),
            machine: name.clone(),
            session: SESSION.to_owned(),
            enabled: true,
        };
        cloud::save(device.clone())?;
        Ok(device)
    });
    if let Err(error) = &ready {
        if let Err(cleanup) = api.delete_sandbox(&sandbox.id) {
            tracing::warn!(category = "opensandbox", id = sandbox.id, error = %cleanup, "Could not delete a failed sandbox");
        }
        tracing::warn!(category = "opensandbox", id = sandbox.id, %error, "Creating a sandbox failed");
    }
    ready
}

fn prepare(
    api: &Api,
    settings: &Settings,
    id: &str,
    approved: bool,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<()> {
    report(cloud::Step::Starting);
    api.wait_running(id, cancelled)?;
    report(cloud::Step::CheckingHerdr);
    if !install::installed(relay_command(settings, id)?, &cancelled)? {
        if !approved {
            return Err(cloud::Error::Install(install::Failure::NotInstalled {
                machine: Some(id.to_string()),
                retry: install::Retry::Allow,
            })
            .into());
        }
        report(cloud::Step::InstallingHerdr);
        install::install(relay_command(settings, id)?, &cancelled)?;
        if !install::installed(relay_command(settings, id)?, &cancelled)? {
            return Err(cloud::Error::Install(install::Failure::Missing).into());
        }
    }
    // Commands such as Teleport's need the server the entrypoint starts.
    let deadline = Instant::now() + SERVER_TIMEOUT;
    while !succeeds(relay_command(settings, id)?, SERVER_UP, cancelled) {
        if cancelled() {
            return Err(super::Error::Cancelled.into());
        }
        if Instant::now() >= deadline {
            tracing::warn!(category = "opensandbox", id, "Herdr's server did not start");
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

/// Whether `script`, run through `relay`, exits 0 within a few seconds.
fn succeeds(mut relay: Command, script: &str, cancelled: &dyn Fn() -> bool) -> bool {
    relay
        .arg(install::remote(script))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = relay.spawn() else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline && !cancelled() => {
                std::thread::sleep(Duration::from_millis(100));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// Make the sandbox `spec` describes on the configured server, for `cloud
/// create` or work that needs a new machine of its own. Sandbox images are
/// not built for OpenSandbox yet, so `spec.image` is ignored. Blocks: run it
/// on a worker.
pub fn provision(
    spec: &Spec,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    let settings = crate::config::load()?
        .opensandbox
        .settings()?
        .ok_or(super::Error::Account)?;
    add_device(
        AddRequest {
            settings,
            name: spec.name.clone().unwrap_or_else(random_name),
            install: spec.install,
        },
        cancelled,
        report,
    )
}

/// Where the API key in use comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Missing,
    Saved,
    Environment,
}

/// What the OpenSandbox tab shows about a server.
#[derive(Debug)]
pub struct Overview {
    pub key: Key,
    /// How many sandboxes this app made that the server lists, or why that
    /// could not be read.
    pub sandboxes: Option<Result<usize>>,
    pub devices: Vec<SavedDevice>,
}

pub fn overview(settings: &Settings) -> Result<Overview> {
    let devices = cloud::load()?
        .into_iter()
        .filter(|saved| {
            saved.provider == CloudProvider::OpenSandbox && saved.account == settings.base
        })
        .collect();
    let key = if store::from_env() {
        Key::Environment
    } else if store::saved(settings.store)?.is_some() {
        Key::Saved
    } else {
        Key::Missing
    };
    let sandboxes = (key != Key::Missing).then(|| {
        let key = store::key(settings)?;
        Api {
            settings,
            key: &key,
        }
        .ours()
        .map(|sandboxes| sandboxes.len())
    });
    Ok(Overview {
        key,
        sandboxes,
        devices,
    })
}

/// Save (or with `None`, forget) the API key typed into Settings.
pub fn save_key(store: Store, key: Option<&SecretString>) -> Result<()> {
    store::save(store, key)
}

/// Forget one saved device; its sandbox is left as it is.
pub fn forget_device(id: &str) -> Result<()> {
    Ok(cloud::remove(CloudProvider::OpenSandbox, id)?)
}

/// Delete the saved sandbox `id`, then forget it. A device no longer saved
/// has nothing left to do. Blocks: run it on a worker.
pub fn destroy_saved(id: &str) -> crate::Result<()> {
    let Some(device) = cloud::load()?
        .into_iter()
        .find(|device| device.provider == CloudProvider::OpenSandbox && device.id == id)
    else {
        return Ok(());
    };
    let settings = super::connect::settings(&device.account)?;
    let key = store::key(&settings)?;
    Api {
        settings: &settings,
        key: &key,
    }
    .delete_sandbox(id)?;
    Ok(cloud::remove(CloudProvider::OpenSandbox, id)?)
}
