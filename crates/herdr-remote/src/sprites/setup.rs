//! The blocking jobs behind the Sprites tab: make a sprite, install Herdr in
//! it on approval, register Herdr's server as its service, and save the
//! device; read what the tab shows; and delete a sprite.

use super::{Result, Settings, connect::relay_command, http::Request, store};
use crate::{
    cloud::{self, CloudProvider, SavedDevice, Spec, install},
    credentials::Store,
};
use secrecy::SecretString;

/// The session every sprite device attaches to.
const SESSION: &str = "default";
/// Sprite names this app makes start with this, so the tab can count them.
const PREFIX: &str = "herdr";
const NAME_LIMIT: usize = 40;
/// How many times the first probe is tried while a new sprite starts.
const PROBE_ATTEMPTS: usize = 6;

/// What adding a sprite needs.
pub struct AddRequest {
    pub settings: Settings,
    pub name: String,
    /// The user approved running Herdr's installer in the sprite.
    pub install: bool,
}

pub fn random_name() -> String {
    cloud::names::random(PREFIX, NAME_LIMIT)
}

/// The whole job for `cloud::Jobs`: make the sprite, install Herdr if
/// approved, run its server as a service, and save the device. A failure
/// after the sprite exists deletes it again, so nothing bills unlisted.
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
    let token = store::token(&settings)?;
    let api = Request {
        settings: &settings,
        token: &token,
    };
    report(cloud::Step::Creating);
    let sprite = api.create(&name)?;
    let ready = prepare(&api, &settings, &sprite.name, install, cancelled, report).and_then(|()| {
        let device = SavedDevice {
            provider: CloudProvider::Sprites,
            id: sprite.name.clone(),
            label: sprite.name.clone(),
            account: settings.base.clone(),
            machine: sprite.name.clone(),
            session: SESSION.to_owned(),
            enabled: true,
        };
        // An unsaved sprite would bill unlisted, so it goes too.
        cloud::save(device.clone())?;
        Ok(device)
    });
    if let Err(error) = &ready {
        if let Err(cleanup) = api.delete_sprite(&sprite.name) {
            tracing::warn!(category = "sprites", sprite = sprite.name, error = %cleanup, "Could not delete a failed sprite");
        }
        tracing::warn!(category = "sprites", sprite = sprite.name, %error, "Creating a sprite failed");
    }
    ready
}

fn prepare(
    api: &Request,
    settings: &Settings,
    sprite: &str,
    approved: bool,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<()> {
    report(cloud::Step::CheckingHerdr);
    // A new sprite may refuse its first command while it starts.
    let mut attempt = 0;
    let installed = loop {
        attempt += 1;
        match install::installed(relay_command(settings, sprite)?, &cancelled) {
            Ok(installed) => break installed,
            Err(error) if attempt >= PROBE_ATTEMPTS || cancelled() => return Err(error.into()),
            Err(_) => std::thread::sleep(std::time::Duration::from_secs(2)),
        }
    };
    if !installed {
        if !approved {
            return Err(cloud::Error::Install(install::Failure::NotInstalled {
                machine: Some(sprite.to_string()),
                retry: install::Retry::Allow,
            })
            .into());
        }
        report(cloud::Step::InstallingHerdr);
        install::install(relay_command(settings, sprite)?, &cancelled)?;
        if !install::installed(relay_command(settings, sprite)?, &cancelled)? {
            return Err(cloud::Error::Install(install::Failure::Missing).into());
        }
    }
    report(cloud::Step::Starting);
    api.serve_herdr(sprite)?;
    Ok(())
}

/// Make the sprite `spec` describes with the configured account, for `cloud
/// create` or work that needs a new machine of its own. Sprites hold no
/// images, so `spec.image` is ignored. Blocks: run it on a worker.
pub fn provision(
    spec: &Spec,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(cloud::Step),
) -> crate::Result<SavedDevice> {
    let settings = crate::config::load()?
        .sprites
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

/// Where the API token in use comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Missing,
    Saved,
    Environment,
}

/// What the Sprites tab shows about an account.
#[derive(Debug)]
pub struct Overview {
    pub token: Token,
    /// How many sprites this app made that the token sees, or why that could
    /// not be read.
    pub sprites: Option<Result<usize>>,
    pub devices: Vec<SavedDevice>,
}

pub fn overview(settings: &Settings) -> Result<Overview> {
    let devices = cloud::load()?
        .into_iter()
        .filter(|saved| saved.provider == CloudProvider::Sprites && saved.account == settings.base)
        .collect();
    let token = if store::from_env() {
        Token::Environment
    } else if store::saved(settings.store)?.is_some() {
        Token::Saved
    } else {
        Token::Missing
    };
    let sprites = (token != Token::Missing).then(|| {
        let token = store::token(settings)?;
        Request {
            settings,
            token: &token,
        }
        .sprites(PREFIX)
        .map(|sprites| sprites.len())
    });
    Ok(Overview {
        token,
        sprites,
        devices,
    })
}

/// Save (or with `None`, forget) the token typed into Settings.
pub fn save_token(store: Store, token: Option<&SecretString>) -> Result<()> {
    store::save(store, token)
}

/// Forget one saved device; its sprite is left as it is.
pub fn forget_device(id: &str) -> Result<()> {
    Ok(cloud::remove(CloudProvider::Sprites, id)?)
}

/// Delete the saved sprite `id`, then forget it. A device no longer saved has
/// nothing left to do. Blocks: run it on a worker.
pub fn destroy_saved(id: &str) -> crate::Result<()> {
    let Some(device) = cloud::load()?
        .into_iter()
        .find(|device| device.provider == CloudProvider::Sprites && device.id == id)
    else {
        return Ok(());
    };
    let settings = super::connect::settings(&device.account)?;
    let token = store::token(&settings)?;
    Request {
        settings: &settings,
        token: &token,
    }
    .delete_sprite(id)?;
    Ok(cloud::remove(CloudProvider::Sprites, id)?)
}
