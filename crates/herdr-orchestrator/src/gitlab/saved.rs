//! GitLab sign-ins kept by the account menu, one per instance: a pasted
//! personal access token, or an account connected with OAuth (`oauth`). They
//! are kept together in one credential-store record of tab-separated lines,
//! in the desktop keyring, or the private `gitlab-credentials` file when
//! `[gitlab] allow_plaintext_credentials` opts in. Every read and write blocks
//! on the store, so callers run on a worker.

use super::{Error, oauth, valid};
use herdr_credentials::{Entry, Store};
use secrecy::{ExposeSecret, SecretString};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

/// The most instances one record keeps, which also bounds its size.
pub const MAX_HOSTS: usize = 16;
/// A host name with an optional port, as a remote names it.
const HOST_LIMIT: usize = 260;
/// An OAuth application ID, as GitLab shows it.
const APPLICATION_LIMIT: usize = 256;
/// How long before it expires an OAuth access token is renewed.
const RENEW_BEFORE: u64 = 10 * 60;
/// The line marker of an OAuth sign-in, where a pasted line has its token.
const OAUTH: &str = "oauth";

const ENTRY: Entry<Error> = Entry {
    service: "dev.herdr.gpui.gitlab",
    account: "tokens",
    label: "Herdr UI GitLab tokens",
    file: c"gitlab-credentials",
    validate: |record| Saved::parse(record).map(drop),
};

/// Every read-modify-write of the record. Never held across a request to
/// GitLab, so a slow renewal holds up no other instance, save, or forget.
static RECORD: Mutex<()> = Mutex::new(());

/// Each instance's renewal, by lowercase instance: GitLab's refresh tokens
/// are single-use, so two workers renewing one instance at once would leave
/// one holding a refresh token GitLab already spent. Taken before [`RECORD`].
static RENEWALS: Mutex<BTreeMap<String, Arc<Mutex<()>>>> = Mutex::new(BTreeMap::new());

/// Renewed pairs the store refused to keep. GitLab spent the pair the store
/// still holds, so until one of these is written the next lookup uses it
/// and writes it again. Taken after [`RECORD`].
static UNSAVED: Mutex<Vec<Unsaved>> = Mutex::new(Vec::new());

/// A renewed pair not yet kept, and the record it belongs to.
struct Unsaved {
    store: Store,
    directory: PathBuf,
    host: String,
    /// The refresh tokens `renewed` replaces: the one the store held, and
    /// the one GitLab spent on `renewed` when that differs, as it does when
    /// a pair the store refused is renewed again. Another lookup may save
    /// that refused pair meanwhile; it is still the same sign-in.
    spent: Vec<SecretString>,
    renewed: OAuth,
}

impl Unsaved {
    /// Whether `kept` is a pair `renewed` replaces.
    fn replaces(&self, kept: &OAuth) -> bool {
        self.spent
            .iter()
            .any(|spent| spent.expose_secret() == kept.refresh.expose_secret())
    }
}

/// An account connected with OAuth.
#[derive(Clone)]
pub struct OAuth {
    pub access: SecretString,
    /// Renews `access`; GitLab replaces it with each renewal.
    pub refresh: SecretString,
    /// When `access` expires, in seconds since the epoch.
    pub expires_at: u64,
    /// The OAuth application that issued the pair, which renews it.
    pub application: String,
}

impl OAuth {
    /// The access token expires within [`RENEW_BEFORE`] of `now`.
    pub fn due(&self, now: u64) -> bool {
        self.expires_at <= now.saturating_add(RENEW_BEFORE)
    }
}

/// What is kept for one instance.
#[derive(Clone)]
pub enum Kept {
    /// A personal access token pasted in the account menu.
    Token(SecretString),
    OAuth(OAuth),
}

impl Kept {
    /// The token requests carry.
    pub fn token(&self) -> &SecretString {
        match self {
            Self::Token(token) => token,
            Self::OAuth(oauth) => &oauth.access,
        }
    }
}

/// The kept sign-ins, by lowercase instance.
#[derive(Default)]
pub struct Saved(BTreeMap<String, Kept>);

impl Saved {
    /// The token kept for `host`, which may be an OAuth token due for renewal:
    /// [`token`] renews it.
    pub fn get(&self, host: &str) -> Option<&SecretString> {
        self.kept(host).map(Kept::token)
    }

    pub fn kept(&self, host: &str) -> Option<&Kept> {
        self.0.get(&host.to_ascii_lowercase())
    }

    /// The instances a sign-in is kept for, sorted.
    pub fn hosts(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// Keeps a pasted `token` for `host`, or forgets the host's sign-in with
    /// `None`.
    pub fn set(&mut self, host: &str, token: Option<SecretString>) -> Result<(), Error> {
        let kept = match token {
            Some(token) => Some(Kept::Token(
                valid(token.expose_secret().trim()).ok_or(Error::Token)?,
            )),
            None => None,
        };
        self.set_kept(host, kept)
    }

    /// Keeps `kept` for `host`, or forgets the host's sign-in with `None`.
    pub fn set_kept(&mut self, host: &str, kept: Option<Kept>) -> Result<(), Error> {
        let host = normalize_host(host).ok_or_else(|| Error::Host(host.to_owned()))?;
        match kept {
            Some(kept) => {
                if let Kept::OAuth(oauth) = &kept
                    && !valid_application(&oauth.application)
                {
                    return Err(Error::Token);
                }
                if !self.0.contains_key(&host) && self.0.len() >= MAX_HOSTS {
                    return Err(Error::TooManyHosts);
                }
                self.0.insert(host, kept);
            }
            None => {
                self.0.remove(&host);
            }
        }
        Ok(())
    }

    /// Reads a record, refusing one that is not lines of a valid host and
    /// sign-in or that names more than [`MAX_HOSTS`] hosts. A pasted token's
    /// line is `host<TAB>token`; an OAuth sign-in's is `host<TAB>oauth<TAB>
    /// access<TAB>refresh<TAB>expires_at<TAB>application`.
    pub fn parse(record: &SecretString) -> Result<Self, Error> {
        let mut saved = BTreeMap::new();
        for line in record
            .expose_secret()
            .lines()
            .filter(|line| !line.is_empty())
        {
            let fields: Vec<&str> = line.split('\t').collect();
            let host = normalize_host(fields[0]).filter(|normal| normal == fields[0]);
            let kept = match fields[1..] {
                [token] => valid(token).map(Kept::Token),
                [OAUTH, access, refresh, expires_at, application] => {
                    match (valid(access), valid(refresh), expires_at.parse()) {
                        (Some(access), Some(refresh), Ok(expires_at))
                            if valid_application(application) =>
                        {
                            Some(Kept::OAuth(OAuth {
                                access,
                                refresh,
                                expires_at,
                                application: application.to_owned(),
                            }))
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            let (Some(host), Some(kept)) = (host, kept) else {
                return Err(Error::Record);
            };
            saved.insert(host, kept);
        }
        if saved.len() > MAX_HOSTS {
            return Err(Error::Record);
        }
        Ok(Self(saved))
    }

    /// The record as the store keeps it.
    pub(crate) fn record(&self) -> SecretString {
        let mut text = Zeroizing::new(String::new());
        for (host, kept) in &self.0 {
            text.push_str(host);
            text.push('\t');
            match kept {
                Kept::Token(token) => text.push_str(token.expose_secret()),
                Kept::OAuth(oauth) => {
                    for field in [
                        OAUTH,
                        oauth.access.expose_secret(),
                        oauth.refresh.expose_secret(),
                        &oauth.expires_at.to_string(),
                        &oauth.application,
                    ] {
                        text.push_str(field);
                        text.push('\t');
                    }
                    text.pop();
                }
            }
            text.push('\n');
        }
        SecretString::from(text.as_str())
    }
}

/// An application ID is a short run of letters, digits, `-`, and `_`.
fn valid_application(application: &str) -> bool {
    !application.is_empty()
        && application.len() <= APPLICATION_LIMIT
        && application
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// A host as the account menu and remotes name it: lowercase, its port
/// kept, and any `https://` or trailing `/` a pasted address carries dropped.
pub fn normalize_host(input: &str) -> Option<String> {
    let host = super::configured_host(input).to_ascii_lowercase();
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) => (name, Some(port)),
        None => (host.as_str(), None),
    };
    let name_ok = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        && !name.starts_with(['.', '-']);
    let port_ok = port.is_none_or(|port| port.parse::<u16>().is_ok_and(|port| port > 0));
    (name_ok && port_ok && host.len() <= HOST_LIMIT).then_some(host)
}

/// Seconds since the epoch, which OAuth expiries count in.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// The folder the GUI config, and so the private credential file, lives in.
fn directory() -> Result<PathBuf, Error> {
    Ok(herdr_client::local_path::config_root()
        .map_err(Error::Directory)?
        .join("herdr"))
}

/// Every kept sign-in in `store`.
pub fn read(store: Store) -> Result<Saved, Error> {
    read_in(store, &directory()?)
}

/// [`read`] with the private file in `directory`.
pub fn read_in(store: Store, directory: &Path) -> Result<Saved, Error> {
    match herdr_credentials::read_entry(store, &ENTRY, directory)? {
        Some(record) => Saved::parse(&record),
        None => Ok(Saved::default()),
    }
}

/// The token to read `host` as from `store`: a pasted one as kept, an OAuth
/// one renewed first when it is about to expire. A renewal that fails keeps
/// the old token, which GitLab refuses once it expires, so the view asks the
/// user to connect again. Blocks on the store and the network.
pub fn token(store: Store, host: &str) -> Result<Option<SecretString>, Error> {
    token_in(store, &directory()?, host, now(), |kept, now| {
        oauth::renew(host, kept, now)
    })
}

/// [`token`] with the private file in `directory`, the time, and the renewal.
pub fn token_in(
    store: Store,
    directory: &Path,
    host: &str,
    now: u64,
    renew: impl FnOnce(&OAuth, u64) -> Result<OAuth, Error>,
) -> Result<Option<SecretString>, Error> {
    let host = host.to_ascii_lowercase();
    if let Current::Ready(token) = current(store, directory, &host, now)? {
        return Ok(token);
    }
    let renewal = Arc::clone(
        RENEWALS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(host.clone())
            .or_default(),
    );
    let _renewal = renewal.lock().unwrap_or_else(PoisonError::into_inner);
    // Look again: another worker may have renewed it while this one waited.
    let (spent, oauth) = match current(store, directory, &host, now)? {
        Current::Ready(token) => return Ok(token),
        Current::Due { spent, oauth } => (spent, oauth),
    };
    tracing::info!(
        category = "orchestrator_gitlab",
        "Renewing a GitLab sign-in"
    );
    match renew(&oauth, now) {
        Ok(renewed) => {
            let access = renewed.access.clone();
            keep_renewed(Unsaved {
                store,
                directory: directory.to_owned(),
                host,
                spent,
                renewed,
            });
            Ok(Some(access))
        }
        Err(error) => {
            tracing::warn!(
                category = "orchestrator_gitlab",
                %error,
                "A GitLab sign-in could not be renewed"
            );
            Ok(Some(oauth.access))
        }
    }
}

/// What the store holds for an instance, read under [`RECORD`].
enum Current {
    Ready(Option<SecretString>),
    /// An OAuth sign-in to renew: `oauth`, the store's pair or a renewal of
    /// it the store refused. `spent` holds the refresh tokens the renewal
    /// replaces, as [`Unsaved::spent`] does.
    Due {
        spent: Vec<SecretString>,
        oauth: OAuth,
    },
}

fn current(store: Store, directory: &Path, host: &str, now: u64) -> Result<Current, Error> {
    let _record = RECORD.lock().unwrap_or_else(PoisonError::into_inner);
    let saved = read_in(store, directory)?;
    let kept = match saved.kept(host) {
        Some(Kept::OAuth(kept)) => kept,
        other => return Ok(Current::Ready(other.map(Kept::token).cloned())),
    };
    let oauth = unsaved(store, directory, host, kept).unwrap_or_else(|| kept.clone());
    Ok(if oauth.due(now) {
        let mut spent = vec![kept.refresh.clone()];
        if oauth.refresh.expose_secret() != kept.refresh.expose_secret() {
            spent.push(oauth.refresh.clone());
        }
        Current::Due { spent, oauth }
    } else {
        Current::Ready(Some(oauth.access))
    })
}

/// The pair renewed from `kept` that the store refused, written again now.
/// Call it with [`RECORD`] held.
fn unsaved(store: Store, directory: &Path, host: &str, kept: &OAuth) -> Option<OAuth> {
    let mut unsaved = UNSAVED.lock().unwrap_or_else(PoisonError::into_inner);
    let index = unsaved.iter().position(|entry| {
        entry.store == store && entry.directory == directory && entry.host == host
    })?;
    let entry = unsaved.swap_remove(index);
    // The sign-in changed since, as a Disconnect or a new Connect does.
    if !entry.replaces(kept) {
        return None;
    }
    let renewed = entry.renewed.clone();
    if let Err(error) = write_in(store, directory, host, Some(Kept::OAuth(renewed.clone()))) {
        tracing::warn!(
            category = "orchestrator_gitlab",
            %error,
            "A renewed GitLab sign-in still could not be saved"
        );
        unsaved.push(entry);
    }
    Some(renewed)
}

/// Keeps `entry`'s renewed pair, unless the sign-in changed while GitLab
/// renewed it. A store that refuses it leaves it in [`UNSAVED`], since GitLab
/// already spent the pair the store holds.
fn keep_renewed(entry: Unsaved) {
    let Unsaved {
        store,
        directory,
        host,
        renewed,
        ..
    } = &entry;
    let _record = RECORD.lock().unwrap_or_else(PoisonError::into_inner);
    let saved = match read_in(*store, directory) {
        Ok(saved) => saved,
        Err(error) => return keep_unsaved(entry, &error),
    };
    if !matches!(saved.kept(host), Some(Kept::OAuth(kept)) if entry.replaces(kept)) {
        return;
    }
    if let Err(error) = write_in(*store, directory, host, Some(Kept::OAuth(renewed.clone()))) {
        keep_unsaved(entry, &error);
    }
}

fn keep_unsaved(entry: Unsaved, error: &Error) {
    tracing::warn!(
        category = "orchestrator_gitlab",
        %error,
        "A renewed GitLab sign-in could not be saved; it is kept until it can be"
    );
    let mut unsaved = UNSAVED.lock().unwrap_or_else(PoisonError::into_inner);
    unsaved.retain(|kept| {
        !(kept.store == entry.store && kept.directory == entry.directory && kept.host == entry.host)
    });
    unsaved.push(entry);
}

/// Saves a pasted `token` for `host` in `store`, or forgets the host's
/// sign-in with `None`, keeping the other hosts'. A record not in the
/// expected form is replaced: none of its tokens could be read, and keeping
/// it would refuse every save and forget with no way out from the app. A
/// store that cannot be read at all still fails, so a locked keyring never
/// loses tokens.
pub fn save(store: Store, host: &str, token: Option<SecretString>) -> Result<(), Error> {
    save_in(store, &directory()?, host, token)
}

/// [`save`] with the private file in `directory`.
pub fn save_in(
    store: Store,
    directory: &Path,
    host: &str,
    token: Option<SecretString>,
) -> Result<(), Error> {
    let kept = match token {
        Some(token) => Some(Kept::Token(
            valid(token.expose_secret().trim()).ok_or(Error::Token)?,
        )),
        None => None,
    };
    save_kept_in(store, directory, host, kept)
}

/// [`save`] for any sign-in, such as an account connected with OAuth.
pub fn save_kept(store: Store, host: &str, kept: Option<Kept>) -> Result<(), Error> {
    save_kept_in(store, &directory()?, host, kept)
}

/// [`save_kept`] with the private file in `directory`.
pub fn save_kept_in(
    store: Store,
    directory: &Path,
    host: &str,
    kept: Option<Kept>,
) -> Result<(), Error> {
    let _record = RECORD.lock().unwrap_or_else(PoisonError::into_inner);
    write_in(store, directory, host, kept)
}

/// The read-modify-write [`save_kept_in`] and a renewal share. Call it with
/// [`RECORD`] held.
fn write_in(store: Store, directory: &Path, host: &str, kept: Option<Kept>) -> Result<(), Error> {
    let mut saved = match read_in(store, directory) {
        Err(Error::Record) => {
            tracing::warn!(
                category = "orchestrator_gitlab",
                "Replacing saved GitLab tokens not in the expected form"
            );
            Saved::default()
        }
        read => read?,
    };
    saved.set_kept(host, kept)?;
    let record = (!saved.0.is_empty()).then(|| saved.record());
    herdr_credentials::save_entry(store, &ENTRY, directory, record.as_ref())
}
