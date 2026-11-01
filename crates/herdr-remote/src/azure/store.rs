//! The Azure Service Principal client secret, kept in the macOS Keychain,
//! Linux Secret Service, or with an explicit opt-in a private file.
//! `HERDR_AZURE_CLIENT_SECRET` takes precedence.

use super::{Error, Result, Settings};
use crate::cloud::Value;
use crate::credentials::{Entry, Store, read_entry, save_entry};
use secrecy::{ExposeSecret, SecretString};

pub const VARIABLE: &str = "HERDR_AZURE_CLIENT_SECRET";
const SECRET_LIMIT: usize = 512;

fn valid(secret: &str) -> bool {
    !secret.is_empty()
        && secret.len() <= SECRET_LIMIT
        && secret.bytes().all(|b| b.is_ascii_graphic())
}

const ENTRY: Entry = Entry {
    service: "dev.herdr.gpui.azure",
    account: "client-secret",
    label: "Herdr GPUI Azure client secret",
    file: c"azure-client-secret",
    validate: |value| {
        if valid(value.expose_secret()) {
            Ok(())
        } else {
            Err(Error::Field(Value::AzureClientSecret).into())
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

pub fn secret(settings: &Settings) -> Result<Option<SecretString>> {
    if let Some(sec) = from_environment()? {
        return Ok(Some(sec));
    }
    saved(settings.store)
}

pub fn saved(store: Store) -> Result<Option<SecretString>> {
    read_entry(store, &ENTRY).map_err(storage)
}

pub fn save(store: Store, secret: Option<&SecretString>) -> Result<()> {
    if secret.is_some_and(|s| !valid(s.expose_secret())) {
        return Err(Error::Field(Value::AzureClientSecret));
    }
    save_entry(store, &ENTRY, secret).map_err(storage)
}
