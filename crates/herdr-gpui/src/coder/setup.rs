//! The blocking jobs behind "Add Coder Workspace": sign in, list what the
//! account can use, create or attach a workspace, wait for it, check for (and
//! on approval install) Herdr, and save the device. Each runs on a worker.

use super::{
    Error, Result, SavedWorkspace, Settings,
    api::{self, Client, Preset, Progress, Template, User, Workspace},
    catalog, connect, install, oauth, store,
};
use secrecy::SecretString;

pub(crate) use oauth::Pending;

/// What the account can use, shown once signed in.
#[derive(Clone, Debug)]
pub(crate) struct Account {
    pub(crate) user: User,
    pub(crate) templates: Vec<Template>,
    pub(crate) workspaces: Vec<Workspace>,
    /// IDs of this deployment's workspaces already saved as devices.
    pub(crate) saved: Vec<String>,
}

/// Where a new device comes from.
#[derive(Clone, Debug)]
pub(crate) enum Source {
    New {
        name: String,
        template: Template,
        preset: Option<Preset>,
    },
    Existing {
        id: String,
        name: String,
    },
}

/// A workspace whose agent accepts `coder ssh`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ready {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) agent: String,
}

/// A step of `provision`, for the dialog's status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Creating,
    Waiting(Progress),
    CheckingHerdr,
}

fn tokens(settings: &Settings) -> impl Fn(bool) -> Result<SecretString> + '_ {
    |rejected| store::current_token(settings, rejected)
}

/// Bind the redirect listener and build the page to open in the browser.
pub(crate) fn begin_sign_in(settings: &Settings) -> Result<Pending> {
    oauth::begin(settings)
}

/// Wait for the browser, save the sign-in, and list what the account can use.
pub(crate) fn finish_sign_in(
    settings: &Settings,
    pending: Pending,
    cancelled: impl Fn() -> bool,
) -> Result<Account> {
    let credential = pending.finish(settings, &cancelled)?;
    store::save(settings, &credential)?;
    account(settings)
}

pub(crate) fn signed_in(settings: &Settings) -> Result<bool> {
    store::signed_in(settings)
}

/// Forget the saved sign-in. The token is revoked at Coder first when possible;
/// a failed revocation never keeps a local sign-in the user asked to remove.
pub(crate) fn sign_out(settings: &Settings) -> Result<()> {
    if let Ok(token) = store::current_token(settings, false) {
        let mut url = url::Url::parse(&settings.endpoint("/oauth2/tokens"))
            .map_err(|_| Error::Url("coder.url"))?;
        url.query_pairs_mut()
            .append_pair("client_id", &settings.client_id);
        if let Err(error) = super::http::delete("oauth2_revoke", &token, url.as_str()) {
            tracing::warn!(category = "coder_signout", %error, "Could not revoke the Coder token");
        }
    }
    store::remove(settings)
}

pub(crate) fn account(settings: &Settings) -> Result<Account> {
    let saved = catalog::load()?
        .into_iter()
        .filter(|saved| saved.deployment == settings.base)
        .map(|saved| saved.id)
        .collect();
    let tokens = tokens(settings);
    store::with_token(&tokens, |token| {
        let client = Client::new(settings, token);
        Ok(Account {
            user: client.me()?,
            templates: client.templates()?,
            workspaces: client.workspaces()?,
            saved: Vec::new(),
        })
    })
    .map(|account| Account { saved, ..account })
}

/// Forget a saved device; the workspace itself is left untouched in Coder.
pub(crate) fn forget(settings: &Settings, id: &str) -> Result<Account> {
    catalog::remove(id)?;
    account(settings)
}

pub(crate) fn presets(settings: &Settings, template: &Template) -> Result<Vec<Preset>> {
    store::with_token(&tokens(settings), |token| {
        Client::new(settings, token).presets(template)
    })
}

fn ssh(settings: &Settings, ready: &Ready) -> Result<std::process::Command> {
    let token = store::current_token(settings, false)?;
    connect::ssh_command(
        &connect::cli(settings)?,
        settings,
        &token,
        &ready.name,
        &ready.agent,
    )
}

/// Create or attach, wait until the agent is ready, and report whether Herdr
/// is already installed there.
pub(crate) fn provision(
    settings: &Settings,
    source: Source,
    cancelled: impl Fn() -> bool,
    mut step: impl FnMut(Step),
) -> Result<(Ready, bool)> {
    // Fail before creating anything when the transport cannot run later.
    connect::cli(settings)?;
    let tokens = tokens(settings);
    let (id, name) = match source {
        Source::Existing { id, name } => (id, name),
        Source::New {
            name,
            template,
            preset,
        } => {
            step(Step::Creating);
            let created = store::with_token(&tokens, |token| {
                Client::new(settings, token).create(&name, &template, preset.as_ref())
            })?;
            (created.id, created.name)
        }
    };
    let (_, agent) = api::wait_ready(settings, &tokens, &id, &cancelled, |progress| {
        step(Step::Waiting(progress));
    })?;
    let ready = Ready { id, name, agent };
    step(Step::CheckingHerdr);
    let installed = install::installed(ssh(settings, &ready)?, &cancelled)?;
    Ok((ready, installed))
}

pub(crate) fn install(
    settings: &Settings,
    ready: &Ready,
    cancelled: impl Fn() -> bool,
) -> Result<()> {
    install::install(ssh(settings, ready)?, &cancelled)?;
    // The installer's own success is not proof the bridge will find it.
    if !install::installed(ssh(settings, ready)?, &cancelled)? {
        return Err(Error::Install(
            "the installer finished but Herdr is not on the expected paths".into(),
        ));
    }
    Ok(())
}

/// Save the device; the endpoint list picks it up on its next catalog read.
pub(crate) fn save(
    settings: &Settings,
    ready: &Ready,
    label: &str,
    session: &str,
) -> Result<SavedWorkspace> {
    let workspace = SavedWorkspace {
        id: ready.id.clone(),
        label: label.trim().to_owned(),
        deployment: settings.base.clone(),
        name: ready.name.clone(),
        session: session.to_owned(),
        enabled: true,
    };
    catalog::save(workspace.clone())?;
    Ok(workspace)
}
