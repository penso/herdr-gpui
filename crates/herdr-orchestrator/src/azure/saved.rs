//! Azure DevOps personal access tokens pasted in the account menu, one per
//! organization. They are kept together in one credential-store record of
//! `organization<TAB>token` lines, in the desktop keyring, or the private
//! `azure-devops-credentials` file when `[azure_devops]
//! allow_plaintext_credentials` opts in. Every read and write blocks on the
//! store, so callers run on a worker.

use super::Error;
use herdr_credentials::{Entry, Store};
use secrecy::{ExposeSecret, SecretString};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};
use zeroize::Zeroizing;

/// The most organizations one record keeps, which also bounds its size.
pub const MAX_ORGANIZATIONS: usize = 32;
/// Azure DevOps organization names are at most 50 characters.
const NAME_LIMIT: usize = 50;

const ENTRY: Entry<Error> = Entry {
    service: "dev.herdr.gpui.azure-devops",
    account: "tokens",
    label: "Herdr UI Azure DevOps tokens",
    file: c"azure-devops-credentials",
    validate: |record| Saved::parse(record).map(drop),
};

/// Every read-modify-write of the record, so windows saving at once take
/// turns rather than losing each other's organizations.
static RECORD: Mutex<()> = Mutex::new(());

/// The kept tokens, by lowercase organization.
#[derive(Default)]
pub struct Saved(BTreeMap<String, SecretString>);

impl Saved {
    /// The token kept for `organization`.
    pub fn get(&self, organization: &str) -> Option<&SecretString> {
        self.0.get(&organization.to_ascii_lowercase())
    }

    /// The organizations a token is kept for, sorted.
    pub fn organizations(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// Keeps `token` for `organization`, or forgets the organization's with
    /// `None`.
    pub fn set(&mut self, organization: &str, token: Option<SecretString>) -> Result<(), Error> {
        let organization = normalize_organization(organization)
            .ok_or_else(|| Error::Organization(organization.to_owned()))?;
        match token {
            Some(token) => {
                let token =
                    super::token::valid(token.expose_secret().trim()).ok_or(Error::Token)?;
                if !self.0.contains_key(&organization) && self.0.len() >= MAX_ORGANIZATIONS {
                    return Err(Error::TooManyOrganizations);
                }
                self.0.insert(organization, token);
            }
            None => {
                self.0.remove(&organization);
            }
        }
        Ok(())
    }

    /// Reads a record, refusing one that is not lines of a valid
    /// organization and token, or that names more than
    /// [`MAX_ORGANIZATIONS`].
    pub fn parse(record: &SecretString) -> Result<Self, Error> {
        let mut saved = BTreeMap::new();
        for line in record
            .expose_secret()
            .lines()
            .filter(|line| !line.is_empty())
        {
            let Some((organization, token)) = line.split_once('\t') else {
                return Err(Error::Record);
            };
            let organization =
                normalize_organization(organization).filter(|normal| normal == organization);
            let (Some(organization), Some(token)) = (organization, super::token::valid(token))
            else {
                return Err(Error::Record);
            };
            saved.insert(organization, token);
        }
        if saved.len() > MAX_ORGANIZATIONS {
            return Err(Error::Record);
        }
        Ok(Self(saved))
    }

    /// The record as the store keeps it.
    fn record(&self) -> SecretString {
        let mut text = Zeroizing::new(String::new());
        for (organization, token) in &self.0 {
            text.push_str(organization);
            text.push('\t');
            text.push_str(token.expose_secret());
            text.push('\n');
        }
        SecretString::from(text.as_str())
    }
}

/// An organization as the account menu and the store name it: lowercase,
/// from a bare name or a pasted `https://dev.azure.com/<organization>` or
/// `https://<organization>.visualstudio.com` address. Azure DevOps names are
/// letters, digits, and hyphens, starting and ending with a letter or digit.
pub fn normalize_organization(input: &str) -> Option<String> {
    let input = input.trim();
    let name = if input.contains(['.', '/', ':']) {
        let address = if input.contains("://") {
            input.to_owned()
        } else {
            format!("https://{input}")
        };
        let url = url::Url::parse(&address).ok()?;
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let host = url.host_str()?.to_ascii_lowercase();
        if host == super::HOST {
            url.path_segments()?
                .find(|segment| !segment.is_empty())?
                .to_owned()
        } else {
            host.strip_suffix(".visualstudio.com")?.to_owned()
        }
    } else {
        input.to_owned()
    };
    let name = name.to_ascii_lowercase();
    let valid = !name.is_empty()
        && name.len() <= NAME_LIMIT
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    valid.then_some(name)
}

/// The folder the GUI config, and so the private credential file, lives in.
fn directory() -> Result<PathBuf, Error> {
    Ok(herdr_client::local_path::config_root()
        .map_err(Error::Directory)?
        .join("herdr"))
}

/// Every kept token in `store`.
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

/// The token kept for `organization` in `store`. A store that cannot be
/// read is logged and skipped, so the Azure CLI's sign-in still serves.
pub fn token(store: Store, organization: &str) -> Option<SecretString> {
    token_in(store, directory().ok()?.as_path(), organization)
}

/// [`token`] with the private file in `directory`.
pub fn token_in(store: Store, directory: &Path, organization: &str) -> Option<SecretString> {
    match read_in(store, directory) {
        Ok(saved) => saved.get(organization).cloned(),
        Err(error) => {
            tracing::warn!(
                category = "orchestrator_azure",
                %error,
                "Saved Azure DevOps tokens could not be read"
            );
            None
        }
    }
}

/// Saves `token` for `organization` in `store`, or forgets the
/// organization's with `None`, keeping the others'. A record not in the
/// expected form is replaced: none of its tokens could be read, and keeping
/// it would refuse every save and forget with no way out from the app. A
/// store that cannot be read at all still fails, so a locked keyring never
/// loses tokens.
pub fn save(store: Store, organization: &str, token: Option<SecretString>) -> Result<(), Error> {
    save_in(store, &directory()?, organization, token)
}

/// [`save`] with the private file in `directory`.
pub fn save_in(
    store: Store,
    directory: &Path,
    organization: &str,
    token: Option<SecretString>,
) -> Result<(), Error> {
    let _record = RECORD.lock().unwrap_or_else(PoisonError::into_inner);
    let mut saved = match read_in(store, directory) {
        Err(Error::Record) => {
            tracing::warn!(
                category = "orchestrator_azure",
                "Replacing saved Azure DevOps tokens not in the expected form"
            );
            Saved::default()
        }
        read => read?,
    };
    saved.set(organization, token)?;
    let record = (!saved.0.is_empty()).then(|| saved.record());
    herdr_credentials::save_entry(store, &ENTRY, directory, record.as_ref())
}
