//! Creating the machines of a confirmed batch, each on its own thread: a
//! provider takes minutes to build one, and one slow machine must not hold
//! up the rest. A machine is reported ready only once it is saved as a
//! device and, when asked, its `[cloud]` setup script has run there without
//! a terminal and exited 0. Each thread reports to the registry itself, so
//! the work finishes and is recorded even if the window that asked closes.

use super::{Error, Registry, Target};
use crate::{
    cloud::{self, CloudProvider, SavedDevice, Spec, exec},
    control::batch::{DeviceInfo, DeviceState},
};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

/// The longest a setup script may run, as installing agents can be slow.
const SETUP_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// The lines of a failed setup script's output kept to say why.
const SETUP_TAIL: usize = 6;

/// What a confirmed create batch makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Creation {
    pub(crate) provider: CloudProvider,
    pub(crate) count: u32,
    pub(crate) spec: Spec,
    /// Whether the `[cloud]` setup script runs before a machine is ready.
    pub(crate) setup: bool,
}

impl Creation {
    /// The batch's devices before they exist, named by their place in it.
    pub(crate) fn devices(&self) -> Vec<DeviceInfo> {
        let (name, noun) = (cloud::name(self.provider), cloud::noun(self.provider));
        (1..=self.count)
            .map(|index| DeviceInfo {
                provider: self.provider.key().to_owned(),
                id: String::new(),
                endpoint: String::new(),
                label: format!("New {name} {noun} {index}"),
                state: DeviceState::Waiting,
                step: None,
                error: None,
            })
            .collect()
    }
}

/// How one device of a batch reports while it is made.
pub(crate) struct Report {
    registry: Registry,
    batch: u64,
    index: usize,
}

impl Report {
    pub(crate) fn step(&self, state: DeviceState, text: String) {
        self.registry.progress(self.batch, self.index, state, text);
    }

    pub(crate) fn created(&self, target: &Target) {
        self.registry.created(self.batch, self.index, target);
    }
}

/// Start making `count` devices of batch `batch`. `work` makes one and
/// reports as it goes; it blocks, off the UI thread.
pub(crate) fn start(
    registry: &Registry,
    batch: u64,
    count: usize,
    work: impl Fn(&Report) -> crate::Result<()> + Send + Sync + 'static,
) {
    let work = Arc::new(work);
    for index in 0..count {
        let report = Report {
            registry: registry.clone(),
            batch,
            index,
        };
        report.step(
            DeviceState::Creating,
            cloud::status::english(&cloud::Step::Creating),
        );
        let work = work.clone();
        let spawned = std::thread::Builder::new()
            .name("herdr-cloud-create".into())
            .spawn(move || match work(&report) {
                Ok(()) => report
                    .registry
                    .update(batch, index, DeviceState::Ready, None),
                Err(error) => {
                    tracing::warn!(
                        category = "cloud_batch",
                        batch,
                        index,
                        %error,
                        "Could not create a cloud device"
                    );
                    report.registry.update(
                        batch,
                        index,
                        DeviceState::Failed,
                        Some(&error.to_string()),
                    );
                }
            });
        if let Err(error) = spawned {
            tracing::error!(category = "cloud_batch", %error, "Could not start a create worker");
            registry.update(
                batch,
                index,
                DeviceState::Failed,
                Some("Herdr UI could not start a worker for it"),
            );
        }
    }
}

/// Make one machine of `creation`, save it, and run the setup script on it
/// when asked. Blocks.
pub(crate) fn create(creation: &Creation, report: &Report) -> crate::Result<()> {
    let provider = creation.provider;
    let provision = cloud::provision(provider).ok_or(Error::NotOffered(cloud::name(provider)))?;
    let device = provision(&creation.spec, &|| false, &|step| {
        report.step(DeviceState::Creating, cloud::status::english(&step));
    })
    .map_err(crate::Error::from)
    .inspect_err(|error| {
        if let Some(device) = saved_before(provider, error) {
            report.created(&target(&device));
        }
    })?;
    report.created(&target(&device));
    if !creation.setup {
        return Ok(());
    }
    let Some(script) = crate::config::Config::load()?.cloud.script(provider)? else {
        return Ok(());
    };
    report.step(DeviceState::SettingUp, "Running the setup script…".into());
    let machine = exec::Machine {
        provider,
        account: device.account,
        id: device.id,
    };
    let finished = exec::run(
        &machine,
        &setup_command(provider, script),
        None,
        SETUP_TIMEOUT,
        &AtomicBool::new(false),
    )
    .map_err(Error::Exec)?;
    if finished.code == Some(0) {
        return Ok(());
    }
    Err(Error::Setup {
        code: finished.code,
        output: tail(&finished),
    }
    .into())
}

fn target(device: &SavedDevice) -> Target {
    Target {
        provider: device.provider,
        id: device.id.clone(),
        endpoint: device.endpoint_id(),
        label: device.label.clone(),
    }
}

/// The device a failed create had already saved. Boat saves its sandbox as
/// soon as it exists, before it is ready and has Herdr, so the batch must
/// still name it for `cloud destroy --batch` to find it.
fn saved_before(provider: CloudProvider, error: &crate::Error) -> Option<SavedDevice> {
    let id = saved_id(error)?;
    cloud::load()
        .ok()?
        .into_iter()
        .find(|device| device.provider == provider && device.id == id)
}

#[cfg(feature = "boat")]
fn saved_id(error: &crate::Error) -> Option<&str> {
    match error {
        crate::Error::Remote(herdr_remote::Error::Provider(cloud::ProviderError::Boat(
            crate::boat::Error::Setup { id, .. },
        ))) => Some(id),
        _ => None,
    }
}

#[cfg(not(feature = "boat"))]
fn saved_id(_: &crate::Error) -> Option<&str> {
    None
}

/// The setup script as a device's setup tab runs it: in the user's login
/// shell from the home directory, with the provider named.
pub(super) fn setup_command(provider: CloudProvider, script: String) -> Vec<String> {
    vec![
        "env".into(),
        format!("{}={}", crate::cloud_setup::PROVIDER_ENV, provider.key()),
        "/bin/sh".into(),
        "-c".into(),
        r#"cd "$HOME" && exec "${SHELL:-/bin/sh}" -lc "$1""#.into(),
        "herdr-setup".into(),
        script,
    ]
}

/// The last lines a failed script printed, stdout's then stderr's.
fn tail(finished: &exec::Finished) -> String {
    let lines: Vec<&str> = finished
        .stdout
        .lines()
        .chain(finished.stderr.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines[lines.len().saturating_sub(SETUP_TAIL)..].join(" / ")
}

#[cfg(test)]
mod tests;
