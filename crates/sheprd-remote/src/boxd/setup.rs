//! The blocking jobs behind the boxd tab: make a machine, install Herdr in it
//! on approval, and save the device; read what the tab shows; and delete a
//! machine.

use super::{Result, Settings, connect::relay_command, http::Request, store};
use crate::{
    cloud::{self, CloudProvider, SavedDevice, Spec, install},
    credentials::Store,
};
use secrecy::SecretString;

/// The session every boxd device attaches to.
const SESSION: &str = "default";
/// Machine names this app makes start with this.
const PREFIX: &str = "sheprd";
const NAME_LIMIT: usize = 40;
/// How many times the first probe is tried while a new machine boots.
const PROBE_ATTEMPTS: usize = 6;

/// What adding a machine needs.
pub struct AddRequest {
    pub settings: Settings,
    pub name: String,
    /// The user approved running Herdr's installer in the machine.
    pub install: bool,
}

pub fn random_name() -> String {
    cloud::names::random(PREFIX, NAME_LIMIT)
}

/// The whole job for `cloud::Jobs`: make the machine, install Herdr if
/// approved, and save the device. A failure after the machine exists deletes
/// it again, so nothing bills unlisted.
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
    let api = Request {
        settings: &settings,
        key: &key,
    };
    report(cloud::Step::Creating);
    let machine = api.create(&name)?;
    let ready = prepare(&api, &settings, &machine.id, install, cancelled, report).and_then(|()| {
        let device = SavedDevice {
            provider: CloudProvider::Boxd,
            id: machine.id.clone(),
            label: machine.name.clone(),
            account: settings.base.clone(),
            machine: machine.name.clone(),
            session: SESSION.to_owned(),
            enabled: true,
        };
        // An unsaved machine would bill unlisted, so it goes too.
        cloud::save(device.clone())?;
        Ok(device)
    });
    if let Err(error) = &ready {
        if let Err(cleanup) = api.delete(&machine.id) {
            tracing::warn!(category = "boxd", machine = machine.name, error = %cleanup, "Could not delete a failed boxd machine");
        }
        tracing::warn!(category = "boxd", machine = machine.name, %error, "Creating a boxd machine failed");
    }
    ready
}

fn prepare(
    api: &Request,
    settings: &Settings,
    id: &str,
    approved: bool,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<()> {
    report(cloud::Step::Starting);
    api.wait_ready(id, cancelled, |_| {})?;
    report(cloud::Step::CheckingHerdr);
    // A new machine may refuse its first command while it boots.
    let mut attempt = 0;
    let installed = loop {
        attempt += 1;
        match install::installed(relay_command(settings, id)?, &cancelled) {
            Ok(installed) => break installed,
            Err(error) if attempt >= PROBE_ATTEMPTS || cancelled() => return Err(error.into()),
            Err(_) => std::thread::sleep(std::time::Duration::from_secs(2)),
        }
    };
    if !installed {
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
    Ok(())
}

/// Make the machine `spec` describes with the configured account, for `cloud
/// create` or work that needs a new machine of its own. boxd machines start
/// from boxd's own image here, so `spec.image` is ignored. Blocks: run it on
/// a worker.
pub fn provision(
    spec: &Spec,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    let settings = crate::config::load()?
        .boxd
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

/// What the boxd tab shows about an account.
#[derive(Debug)]
pub struct Overview {
    pub key: Key,
    /// How many machines this app made that the key sees, or why that could
    /// not be read.
    pub machines: Option<Result<usize>>,
    pub devices: Vec<SavedDevice>,
}

pub fn overview(settings: &Settings) -> Result<Overview> {
    let devices = cloud::load()?
        .into_iter()
        .filter(|saved| saved.provider == CloudProvider::Boxd && saved.account == settings.base)
        .collect();
    let key = if store::from_env() {
        Key::Environment
    } else if store::saved(settings.store)?.is_some() {
        Key::Saved
    } else {
        Key::Missing
    };
    let machines = (key != Key::Missing).then(|| {
        let key = store::key(settings)?;
        Request {
            settings,
            key: &key,
        }
        .machines()
        .map(|machines| machines.len())
    });
    Ok(Overview {
        key,
        machines,
        devices,
    })
}

/// Save (or with `None`, forget) the API key typed into Settings.
pub fn save_key(store: Store, key: Option<&SecretString>) -> Result<()> {
    store::save(store, key)
}

/// Forget one saved device; its machine is left as it is.
pub fn forget_device(id: &str) -> Result<()> {
    Ok(cloud::remove(CloudProvider::Boxd, id)?)
}

/// Delete the saved machine `id`, then forget it. A device no longer saved
/// has nothing left to do. Blocks: run it on a worker.
pub fn destroy_saved(id: &str) -> crate::Result<()> {
    let Some(device) = cloud::load()?
        .into_iter()
        .find(|device| device.provider == CloudProvider::Boxd && device.id == id)
    else {
        return Ok(());
    };
    let settings = super::connect::settings(&device.account)?;
    let key = store::key(&settings)?;
    Request {
        settings: &settings,
        key: &key,
    }
    .delete(id)?;
    Ok(cloud::remove(CloudProvider::Boxd, id)?)
}
