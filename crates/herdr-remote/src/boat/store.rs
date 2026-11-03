//! The Boat API key, kept in the macOS Keychain, Linux Secret Service, or with
//! an explicit opt-in a private file. `HERDR_BOAT_API_KEY` takes precedence.

use super::{Error, Result, Settings};
use crate::cloud::Value;
use crate::credentials::{Entry, Store, read_entry, save_entry};
use secrecy::{ExposeSecret, SecretString};

pub const VARIABLE: &str = "HERDR_BOAT_API_KEY";
const KEY_LIMIT: usize = 512;

fn valid(key: &str) -> bool {
    !key.is_empty() && key.len() <= KEY_LIMIT && key.bytes().all(|b| b.is_ascii_graphic())
}

const ENTRY: Entry = Entry {
    service: "dev.herdr.gpui.boat",
    account: "api-key",
    label: "Herdr GPUI Boat API key",
    file: c"boat-api-key",
    validate: |value| {
        if valid(value.expose_secret()) {
            Ok(())
        } else {
            Err(Error::Field(Value::BoatApiKey).into())
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

pub fn key(settings: &Settings) -> Result<SecretString> {
    if let Some(key) = from_environment()? {
        return Ok(key);
    }
    saved(settings.store)?.ok_or(Error::MissingKey)
}

pub fn saved(store: Store) -> Result<Option<SecretString>> {
    read_entry(store, &ENTRY).map_err(storage)
}

pub fn save(store: Store, key: Option<&SecretString>) -> Result<()> {
    if key.is_some_and(|key| !valid(key.expose_secret())) {
        return Err(Error::Field(Value::BoatApiKey));
    }
    save_entry(store, &ENTRY, key).map_err(storage)
}
