//! The saved Coder sign-in and the rule for using it: renew before expiry and
//! persist the rotated pair before any other request. All windows and the
//! connection workers share one record, so every access is one transaction.

use super::{Error, Result, Settings, oauth, token::Credential};
use crate::github::{Entry, read_entry, save_entry};
use secrecy::SecretString;
use std::time::SystemTime;

const ENTRY: Entry = Entry {
    service: "dev.herdr.gpui.coder",
    account: "coder",
    label: "Herdr GPUI Coder sign-in",
    file: c"coder-credentials",
    // Decoding bounds the record at `token::LIMIT` bytes.
    validate: |value| {
        Credential::decode(value)
            .map(drop)
            .map_err(crate::Error::from)
    },
};

// A refresh token is single-use; a second worker renewing with it would sign
// the user out. Only background workers call this; never on the UI thread.
fn transaction<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
    static ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ACCESS.lock().unwrap_or_else(|error| error.into_inner());
    work()
}

fn storage(error: crate::Error) -> Error {
    Error::Storage(Box::new(error))
}

fn read(settings: &Settings) -> Result<Option<SecretString>> {
    read_entry(settings.store, &ENTRY).map_err(storage)
}

fn write(settings: &Settings, value: Option<&SecretString>) -> Result<()> {
    save_entry(settings.store, &ENTRY, value).map_err(storage)
}

/// The saved record when it was issued for these settings.
fn issued(value: Option<SecretString>, settings: &Settings) -> Result<Option<Credential>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let credential = Credential::decode(&value)?;
    Ok(credential.issued_for(settings).then_some(credential))
}

pub(crate) fn save(settings: &Settings, credential: &Credential) -> Result<()> {
    transaction(|| write(settings, Some(&credential.encode()?)))
}

pub(crate) fn remove(settings: &Settings) -> Result<()> {
    transaction(|| write(settings, None))
}

/// Whether a sign-in for these settings is saved. Does not contact Coder.
pub(crate) fn signed_in(settings: &Settings) -> Result<bool> {
    transaction(|| Ok(issued(read(settings)?, settings)?.is_some()))
}

/// A current access token from `saved`, renewing first when it is about to
/// expire or when `rejected` says Coder refused the last one. A renewed pair is
/// persisted before it is returned; a dead grant is removed.
fn renewed(
    saved: Option<SecretString>,
    settings: &Settings,
    rejected: bool,
    renew: impl FnOnce(&Settings, &Credential) -> Result<Credential>,
    mut persist: impl FnMut(Option<&SecretString>) -> Result<()>,
) -> Result<SecretString> {
    let saved = issued(saved, settings)?.ok_or(Error::Authentication)?;
    if !rejected && !saved.renewal_due(SystemTime::now()) {
        return Ok(saved.access_token);
    }
    if saved.refresh_token.is_none() {
        return if rejected {
            Err(Error::Authentication)
        } else {
            Ok(saved.access_token)
        };
    }
    let renewed = match renew(settings, &saved) {
        Ok(renewed) => renewed,
        Err(Error::Authentication) => {
            // The grant is gone; keeping it would only fail again.
            persist(None)?;
            return Err(Error::Authentication);
        }
        Err(error) => return Err(error),
    };
    persist(Some(&renewed.encode()?))?;
    Ok(renewed.access_token)
}

/// A token from the configured store, renewing through Coder when due.
pub(crate) fn current_token(settings: &Settings, rejected: bool) -> Result<SecretString> {
    transaction(|| {
        renewed(
            read(settings)?,
            settings,
            rejected,
            oauth::refresh,
            |value| write(settings, value),
        )
    })
}

/// Run `request` with a token from `tokens`, asking once more with
/// `rejected = true` if Coder refuses it.
pub(crate) fn with_token<T>(
    tokens: &impl Fn(bool) -> Result<SecretString>,
    mut request: impl FnMut(&SecretString) -> Result<T>,
) -> Result<T> {
    match request(&tokens(false)?) {
        Err(Error::Authentication) => request(&tokens(true)?),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::coder::tests::settings;
    use secrecy::ExposeSecret;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Memory(RefCell<Option<String>>);

    impl Memory {
        fn saved(&self) -> Option<SecretString> {
            self.0.borrow().as_deref().map(SecretString::from)
        }
        fn token(
            &self,
            settings: &Settings,
            rejected: bool,
            renew: impl FnOnce(&Settings, &Credential) -> Result<Credential>,
        ) -> Result<SecretString> {
            renewed(self.saved(), settings, rejected, renew, |value| {
                *self.0.borrow_mut() = value.map(|v| v.expose_secret().to_owned());
                Ok(())
            })
        }
        fn load(&self) -> Option<Credential> {
            issued(self.saved(), &settings()).unwrap()
        }
    }

    fn credential(access: &str, expires_at: Option<u64>) -> Credential {
        Credential::new(
            &settings(),
            access.into(),
            Some(format!("{access}-refresh").into()),
            expires_at,
        )
        .unwrap()
    }

    fn saved(credential: &Credential) -> Memory {
        Memory(RefCell::new(Some(
            credential.encode().unwrap().expose_secret().to_owned(),
        )))
    }

    #[test]
    fn a_fresh_token_is_used_without_renewal() {
        let vault = saved(&credential("old", None));
        let token = vault
            .token(&settings(), false, |_, _| panic!("no renewal"))
            .unwrap();
        assert_eq!(token.expose_secret(), "old");
    }

    #[test]
    fn an_expiring_token_is_renewed_and_persisted_before_use() {
        let vault = saved(&credential("old", Some(0)));
        let token = vault
            .token(&settings(), false, |_, saved| {
                assert_eq!(
                    saved.refresh_token.as_ref().unwrap().expose_secret(),
                    "old-refresh"
                );
                Ok(credential("new", None))
            })
            .unwrap();
        assert_eq!(token.expose_secret(), "new");
        let stored = vault.load().unwrap();
        assert_eq!(stored.access_token.expose_secret(), "new");
        assert_eq!(stored.refresh_token.unwrap().expose_secret(), "new-refresh");
    }

    #[test]
    fn a_rejected_token_renews_once_and_a_dead_grant_signs_out() {
        let vault = saved(&credential("old", None));
        let calls = Cell::new(0);
        let token = vault
            .token(&settings(), true, |_, _| {
                calls.set(calls.get() + 1);
                Ok(credential("new", None))
            })
            .unwrap();
        assert_eq!((token.expose_secret(), calls.get()), ("new", 1));

        let result = vault.token(&settings(), true, |_, _| Err(Error::Authentication));
        assert!(matches!(result, Err(Error::Authentication)));
        assert!(vault.0.borrow().is_none());
    }

    #[test]
    fn transient_renewal_failures_keep_the_saved_sign_in() {
        let vault = saved(&credential("old", Some(0)));
        let result = vault.token(&settings(), false, |_, _| {
            Err(crate::coder::Status {
                code: 503,
                message: None,
            }
            .into())
        });
        assert!(matches!(result, Err(Error::Status(_))));
        assert!(vault.load().is_some());
    }

    #[test]
    fn another_deployments_sign_in_reads_as_signed_out() {
        let vault = saved(&credential("old", None));
        let mut other = settings();
        other.base = "https://other.example.com".into();
        assert!(issued(vault.saved(), &other).unwrap().is_none());
        assert!(matches!(
            vault.token(&other, false, |_, _| panic!("never sent")),
            Err(Error::Authentication)
        ));
        assert!(vault.load().is_some());
    }
}
