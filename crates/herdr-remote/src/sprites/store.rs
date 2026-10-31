//! The Sprites API token, kept like every other credential this app saves:
//! the macOS Keychain, the Linux Secret Service, or with an explicit opt-in a
//! private file. `HERDR_SPRITES_TOKEN` takes precedence when set.

use super::{Error, Result, Settings};
use crate::cloud::Value;
use crate::credentials::{Entry, Store, read_entry, save_entry};
use secrecy::{ExposeSecret, SecretString};

pub const VARIABLE: &str = "HERDR_SPRITES_TOKEN";
const TOKEN_LIMIT: usize = 1024;

fn valid(token: &str) -> bool {
    !token.is_empty() && token.len() <= TOKEN_LIMIT && token.bytes().all(|b| b.is_ascii_graphic())
}

const ENTRY: Entry = Entry {
    service: "dev.herdr.gpui.sprites",
    account: "api-token",
    label: "Herdr UI Sprites API token",
    file: c"sprites-api-token",
    validate: |value| {
        if valid(value.expose_secret()) {
            Ok(())
        } else {
            Err(Error::Field(Value::SpritesApiToken).into())
        }
    },
};

fn storage(error: crate::Error) -> Error {
    Error::Storage(Box::new(error))
}

fn from_environment() -> Result<Option<SecretString>> {
    let Some(value) = std::env::var_os(VARIABLE) else {
        return Ok(None);
    };
    let value = value
        .into_string()
        .map_err(|_| Error::Encoding(Value::Key(VARIABLE)))?;
    if !valid(value.trim()) {
        return Err(Error::Field(Value::Key(VARIABLE)));
    }
    Ok(Some(value.trim().to_owned().into()))
}

/// The token to use: the environment's, else the saved one. Blocks on the
/// credential store; call from a worker.
pub fn token(settings: &Settings) -> Result<SecretString> {
    if let Some(token) = from_environment()? {
        return Ok(token);
    }
    saved(settings.store)?.ok_or(Error::MissingToken)
}

pub fn saved(store: Store) -> Result<Option<SecretString>> {
    read_entry(store, &ENTRY).map_err(storage)
}

/// Save or, with `None`, forget the token. Blocks; call from a worker.
pub fn save(store: Store, token: Option<&SecretString>) -> Result<()> {
    if token.is_some_and(|token| !valid(token.expose_secret())) {
        return Err(Error::Field(Value::SpritesApiToken));
    }
    save_entry(store, &ENTRY, token).map_err(storage)
}

/// Whether the token comes from the environment.
pub fn from_env() -> bool {
    std::env::var_os(VARIABLE).is_some()
}
