//! The macOS login Keychain, used only by signed release builds: an unsigned
//! build's code identity changes on every rebuild, so macOS would ask again
//! each time.

use crate::{Error, Result, credential_bytes};
use secrecy::{ExposeSecret, SecretString};
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

/// `errSecItemNotFound`.
const NOT_FOUND: i32 = -25300;

pub(crate) fn read(service: &str, account: &str) -> Result<Option<SecretString>> {
    match get_generic_password(service, account) {
        Ok(bytes) => credential_bytes(bytes).map(Some),
        Err(error) if error.code() == NOT_FOUND => Ok(None),
        Err(error) => Err(Error::KeychainRead(error)),
    }
}

pub(crate) fn save(service: &str, account: &str, token: Option<&SecretString>) -> Result<()> {
    let result = match token {
        Some(token) => set_generic_password(service, account, token.expose_secret().as_bytes()),
        None => delete_generic_password(service, account),
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) if token.is_none() && error.code() == NOT_FOUND => Ok(()),
        Err(error) => Err(Error::KeychainWrite(error)),
    }
}
