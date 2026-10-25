#![allow(clippy::unwrap_used)]
use super::*;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Held by each test for its whole run, cleanup included. Credential Manager
/// keeps one store per user, and a write that races another thread's write or
/// delete there can be lost: the next read then finds nothing.
static CREDENTIAL_MANAGER: Mutex<()> = Mutex::new(());

/// A service name of this test run's own, removed again when dropped, so the
/// user's real sign-ins are never touched and a failed test leaves nothing.
struct Scratch {
    service: String,
    accounts: [&'static str; 2],
    // Dropped after `Drop::drop` has removed the entries.
    _serial: MutexGuard<'static, ()>,
}

impl Scratch {
    fn new(test: &str) -> Self {
        Self {
            service: format!("dev.herdr.gpui.test.{test}.{}", std::process::id()),
            accounts: [
                "github.com",
                "github.com/host/0123456789abcdef0123456789abcdef",
            ],
            _serial: CREDENTIAL_MANAGER
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        }
    }

    fn read(&self, account: &str) -> Option<String> {
        read(&self.service, account)
            .unwrap()
            .map(|token| token.expose_secret().to_owned())
    }

    fn save(&self, account: &str, token: Option<&str>) -> Result<()> {
        save(
            &self.service,
            "Herdr UI test",
            token.map(SecretString::from).as_ref(),
            account,
        )
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for account in self.accounts {
            let _ = self.save(account, None);
        }
    }
}

#[test]
fn entries_are_named_by_service_and_account() {
    assert_eq!(
        target("dev.herdr.gpui.github", "github.com/host/0123"),
        "dev.herdr.gpui.github/github.com/host/0123"
    );
    assert_eq!(
        target("dev.herdr.gpui.coder", "coder"),
        "dev.herdr.gpui.coder/coder"
    );
    assert_eq!(wide("ab"), [u16::from(b'a'), u16::from(b'b'), 0]);
}

#[test]
fn saves_reads_replaces_and_removes_an_entry() {
    let scratch = Scratch::new("roundtrip");
    let [main, host] = scratch.accounts;
    assert_eq!(scratch.read(main), None);
    scratch.save(main, Some("first")).unwrap();
    scratch.save(host, Some("elsewhere")).unwrap();
    scratch.save(main, Some("second")).unwrap();
    assert_eq!(scratch.read(main).as_deref(), Some("second"));
    scratch.save(main, None).unwrap();
    assert_eq!(scratch.read(main), None);
    // Removing is idempotent and leaves other accounts alone.
    scratch.save(main, None).unwrap();
    assert_eq!(scratch.read(host).as_deref(), Some("elsewhere"));
}

#[test]
fn a_secret_too_large_for_credential_manager_is_refused_and_keeps_the_old_one() {
    let scratch = Scratch::new("large");
    let [main, _] = scratch.accounts;
    scratch.save(main, Some("kept")).unwrap();
    let largest = "a".repeat(CRED_MAX_CREDENTIAL_BLOB_SIZE as usize);
    scratch.save(main, Some(&largest)).unwrap();
    assert_eq!(scratch.read(main), Some(largest));
    scratch.save(main, Some("kept")).unwrap();
    let error = scratch
        .save(
            main,
            Some(&"a".repeat(CRED_MAX_CREDENTIAL_BLOB_SIZE as usize + 1)),
        )
        .unwrap_err();
    assert!(
        matches!(&error, crate::Error::CredentialManager(Error::Write(source)) if source.kind() == io::ErrorKind::FileTooLarge),
        "{error:?}"
    );
    assert_eq!(scratch.read(main).as_deref(), Some("kept"));
}

#[test]
fn a_saved_entry_that_is_not_utf8_is_an_encoding_error() {
    let scratch = Scratch::new("encoding");
    let [main, _] = scratch.accounts;
    let mut name = wide(&target(&scratch.service, main));
    let mut blob = [0xff_u8, 0xfe];
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: name.as_mut_ptr(),
        CredentialBlobSize: 2,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..CREDENTIALW::default()
    };
    // SAFETY: as in `save`: every pointer outlives the call.
    #[allow(unsafe_code)]
    let written = unsafe { CredWriteW(&credential, 0) };
    assert_ne!(written, 0, "{}", io::Error::last_os_error());
    let result = read(&scratch.service, main);
    assert!(
        matches!(result, Err(crate::Error::Encoding(_))),
        "{result:?}"
    );
}
