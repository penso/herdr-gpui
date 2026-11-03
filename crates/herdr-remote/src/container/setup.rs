//! The blocking jobs behind the Container tab: make a container, install Herdr
//! in it on approval, and save the device; read what the tab shows; and
//! delete a container with its home volume.

use super::{Engine, Result, Settings, machine};
pub use super::{
    machine::State,
    settings::{DEFAULT_CPUS, DEFAULT_IMAGE, DEFAULT_MEMORY},
};
use crate::cloud::Value;
use crate::cloud::{self, CloudProvider, SavedDevice, Spec, install};

/// The session every container device attaches to.
const SESSION: &str = "default";
/// Container names stay short enough to read in the sidebar.
const NAME_LIMIT: usize = 40;

/// What adding a container needs.
pub struct AddRequest {
    pub settings: Settings,
    pub name: String,
    /// The user approved running Herdr's installer in the container.
    pub install: bool,
}

/// Containers being made now, which count against the cap until saved:
/// creations run side by side, and each saves only when it finishes.
static CREATING: std::sync::Mutex<usize> = std::sync::Mutex::new(0);

/// A place under `[container] max_containers`, held while one is made.
struct Slot;

impl Slot {
    /// Take a place if the saved containers and those being made leave one.
    /// `saved` is counted under the lock: a creation saves its device before
    /// it gives its place back, so it is always counted once.
    fn take(saved: impl FnOnce() -> crate::Result<usize>, max: usize) -> crate::Result<Self> {
        let mut creating = CREATING.lock().unwrap_or_else(|error| error.into_inner());
        if saved()? + *creating >= max {
            return Err(super::Error::Limit(max).into());
        }
        *creating += 1;
        Ok(Self)
    }
}

fn saved_containers() -> crate::Result<usize> {
    Ok(cloud::load()?
        .iter()
        .filter(|device| device.provider == CloudProvider::Container)
        .count())
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut creating = CREATING.lock().unwrap_or_else(|error| error.into_inner());
        *creating = creating.saturating_sub(1);
    }
}

/// A default name for a new container, which is also its device ID.
pub fn random_name() -> String {
    cloud::names::random("herdr", NAME_LIMIT)
}

/// The whole job for `cloud::Jobs`: make the container, install Herdr if
/// approved, and save the device. A failure after the container exists
/// deletes it again.
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
    let _slot = Slot::take(saved_containers, settings.max)?;
    let engine = Engine::pick(settings.engine)?;
    report(cloud::Step::Creating);
    machine::create(engine, &name, &settings, cancelled, &|| {
        report(cloud::Step::Building("pulling image".into()));
    })?;
    // An unsaved container would run unlisted, so a failure deletes it too.
    let ready = make_ready(engine, &name, install, cancelled, report);
    if let Err(error) = &ready {
        if let Err(cleanup) = machine::delete(engine, &name, &|| false) {
            tracing::warn!(category = "container", id = name, error = %cleanup, "Could not remove a failed container");
        }
        tracing::warn!(category = "container", id = name, %error, "Creating a container failed");
    }
    ready
}

/// Install Herdr in the running container `id` if needed and approved, start
/// its server, and save it as a device.
fn make_ready(
    engine: Engine,
    id: &str,
    install: bool,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    install_herdr(engine, id, install, cancelled, report)?;
    // Commands such as Teleport's need a running server before any window
    // connects; a failure here still leaves a device a connection can start.
    if let Err(error) = machine::start_server(engine, id, cancelled) {
        tracing::warn!(category = "container", id, %error, "Herdr's server did not start");
    }
    let device = SavedDevice {
        provider: CloudProvider::Container,
        id: id.to_owned(),
        label: id.to_owned(),
        account: engine.key().to_owned(),
        machine: id.to_owned(),
        session: SESSION.to_owned(),
        enabled: true,
    };
    cloud::save(device.clone())?;
    Ok(device)
}

fn install_herdr(
    engine: Engine,
    id: &str,
    approved: bool,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<()> {
    report(cloud::Step::CheckingHerdr);
    if install::installed(machine::exec(engine, id)?, &cancelled)? {
        return Ok(());
    }
    if !approved {
        return Err(cloud::Error::Install(install::Failure::NotInstalled {
            machine: Some(id.to_string()),
            retry: install::Retry::Allow,
        })
        .into());
    }
    report(cloud::Step::InstallingHerdr);
    install::install(machine::exec(engine, id)?, &cancelled)?;
    // The installer's own success is not proof the bridge will find it.
    if !install::installed(machine::exec(engine, id)?, &cancelled)? {
        return Err(cloud::Error::Install(install::Failure::Missing).into());
    }
    Ok(())
}

/// Make the container `spec` describes with the configured settings, for
/// `cloud create` or work that needs a new machine of its own, such as a
/// worktree dispatched to a new container. It starts from `spec.image`, a
/// project's sandbox image, when there is one; Herdr is in it already.
/// Blocks: run it on a worker.
pub fn provision(
    spec: &Spec,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    let mut settings = crate::config::load()?.container.settings()?;
    if let Some(image) = &spec.image {
        image.clone_into(&mut settings.image);
    }
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

/// A saved container and what its engine says about it now.
#[derive(Clone, Debug)]
pub struct Device {
    pub saved: SavedDevice,
    /// `None` when the engine could not be asked.
    pub state: Option<State>,
}

/// What the Container tab shows.
#[derive(Clone, Debug)]
pub struct Overview {
    /// The engine new containers use and its server version, or why there is
    /// none to use.
    pub engine: std::result::Result<(Engine, String), String>,
    pub devices: Vec<Device>,
    /// Containers this app made that are not saved as devices, such as one
    /// whose app quit while it was being made.
    pub unsaved: Vec<String>,
}

pub fn overview(settings: &Settings, cancelled: &dyn Fn() -> bool) -> Result<Overview> {
    let quick = std::time::Duration::from_secs(10);
    let engine = Engine::pick(settings.engine)
        .and_then(|engine| {
            let version = engine.run(
                "version",
                ["version", "--format", "{{.Server.Version}}"],
                quick,
                cancelled,
            )?;
            Ok((engine, version))
        })
        .map_err(|error| error.to_string());
    let devices: Vec<Device> = cloud::load()?
        .into_iter()
        .filter(|saved| saved.provider == CloudProvider::Container)
        .map(|saved| {
            let state = Engine::from_key(&saved.account)
                .and_then(|engine| machine::state(engine, &saved.id, cancelled).ok());
            Device { saved, state }
        })
        .collect();
    let unsaved = match &engine {
        Ok((engine, _)) => machine::labelled(*engine, cancelled)
            .unwrap_or_default()
            .into_iter()
            .filter(|name| !devices.iter().any(|device| &device.saved.id == name))
            // One being made now saves itself when it is ready.
            .filter(|_| *CREATING.lock().unwrap_or_else(|error| error.into_inner()) == 0)
            .collect(),
        Err(_) => Vec::new(),
    };
    Ok(Overview {
        engine,
        devices,
        unsaved,
    })
}

/// Save the container `id`, which this app made on `engine`, as a device
/// again, its files as they were. The app may have quit before Herdr was
/// installed, so the container is started and set up as a new one is: with
/// `install` approved, Herdr is installed if it is missing. Unlike a new
/// container, a failure leaves it as it was. Blocks: run it on a worker.
pub fn adopt(
    engine: Engine,
    id: &str,
    install: bool,
    cancelled: &dyn Fn() -> bool,
) -> crate::Result<SavedDevice> {
    if !machine::labelled(engine, cancelled)?
        .iter()
        .any(|name| name == id)
    {
        return Err(super::Error::Gone(id.to_owned()).into());
    }
    // Adding it back is the user's explicit start, as a reconnect is; it
    // still waits for a stop already under way.
    super::lifecycle::resume_device(engine, id);
    super::lifecycle::start(engine, id, || {
        machine::ensure_running(engine, id, cancelled)
    })?;
    make_ready(engine, id, install, cancelled, &|_| {})
}

/// Delete the saved container `id` with its home volume, then forget it. A
/// device no longer saved has nothing left to do. Blocks: run it on a worker.
pub fn destroy_saved(id: &str) -> crate::Result<()> {
    let Some(device) = cloud::load()?
        .into_iter()
        .find(|device| device.provider == CloudProvider::Container && device.id == id)
    else {
        return Ok(());
    };
    let engine =
        Engine::from_key(&device.account).ok_or(super::Error::Field(Value::ContainerEngine))?;
    machine::delete(engine, id, &|| false)?;
    Ok(cloud::remove(CloudProvider::Container, id)?)
}

/// Stop the saved container `id`, keeping its files for the next connection.
pub fn stop_saved(id: &str) -> crate::Result<()> {
    let Some(device) = cloud::load()?
        .into_iter()
        .find(|device| device.provider == CloudProvider::Container && device.id == id)
    else {
        return Ok(());
    };
    let engine =
        Engine::from_key(&device.account).ok_or(super::Error::Field(Value::ContainerEngine))?;
    Ok(super::lifecycle::stop(engine, id, || {
        machine::stop(engine, id, &|| false)
    })?)
}

#[cfg(test)]
mod tests;
