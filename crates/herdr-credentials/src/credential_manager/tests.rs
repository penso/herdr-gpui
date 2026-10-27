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
    assert_eq!(
        part_target("dev.herdr.gpui.gitlab", "tokens", Set::A, 0),
        "dev.herdr.gpui.gitlab/tokens#a1"
    );
    assert_eq!(
        part_target("dev.herdr.gpui.gitlab", "tokens", Set::B, 2),
        "dev.herdr.gpui.gitlab/tokens#b3"
    );
    let split = Split {
        set: Set::B,
        count: 3,
    };
    assert_eq!(Split::of(split.entry().as_bytes()).unwrap().unwrap(), split);
    assert!(Split::of(b"glpat-token").is_none());
    assert!(Split::of(b"herdr-parts:c:1").unwrap().is_err());
    assert!(Split::of(b"herdr-parts:a:0").unwrap().is_err());
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

/// No part of either set is left for `account`.
fn no_parts(scratch: &Scratch, account: &str) -> bool {
    Set::BOTH.iter().all(|&set| {
        (0..PARTS).all(|index| {
            read_part(&part_target(&scratch.service, account, set, index))
                .unwrap()
                .is_none()
        })
    })
}

#[test]
fn a_secret_too_large_for_one_credential_is_split_and_joined_again() {
    let scratch = Scratch::new("large");
    let [main, _] = scratch.accounts;
    let largest = "a".repeat(PART);
    scratch.save(main, Some(&largest)).unwrap();
    assert_eq!(scratch.read(main), Some(largest));
    // Each new split secret takes the other set of parts.
    for fill in ["abc", "xyz", "abc"] {
        let split = fill.repeat(PART);
        scratch.save(main, Some(&split)).unwrap();
        assert_eq!(scratch.read(main), Some(split));
    }
    // A smaller secret leaves no part of the larger one behind.
    scratch.save(main, Some("kept")).unwrap();
    assert_eq!(scratch.read(main).as_deref(), Some("kept"));
    assert!(no_parts(&scratch, main));
    let error = scratch
        .save(main, Some(&"a".repeat(PART * PARTS + 1)))
        .unwrap_err();
    assert!(
        matches!(&error, crate::Error::CredentialManager(Error::Write(source)) if source.kind() == io::ErrorKind::FileTooLarge),
        "{error:?}"
    );
    assert_eq!(scratch.read(main).as_deref(), Some("kept"));
    // Removing the entry removes every part.
    scratch.save(main, Some(&"abc".repeat(PART))).unwrap();
    scratch.save(main, None).unwrap();
    assert_eq!(scratch.read(main), None);
    assert!(no_parts(&scratch, main));
}

#[test]
fn a_save_stopped_before_its_switch_leaves_the_previous_secret_whole() {
    let scratch = Scratch::new("stopped");
    let [main, _] = scratch.accounts;
    let previous = "p".repeat(PART * 2);
    scratch.save(main, Some(&previous)).unwrap();
    // A save that wrote the other set's parts, then stopped.
    for index in 0..3 {
        let target = part_target(&scratch.service, main, Set::B, index);
        write_part(&target, "Herdr UI test", main, b"half-written").unwrap();
    }
    assert_eq!(scratch.read(main), Some(previous));
    // The next save writes over them, and leaves none behind.
    scratch.save(main, Some("next")).unwrap();
    assert_eq!(scratch.read(main).as_deref(), Some("next"));
    assert!(no_parts(&scratch, main));
}

#[test]
fn a_split_secret_missing_a_part_is_an_error_not_a_shorter_secret() {
    let scratch = Scratch::new("missing");
    let [main, _] = scratch.accounts;
    scratch.save(main, Some(&"m".repeat(PART * 2))).unwrap();
    let entry = read_part(&target(&scratch.service, main)).unwrap().unwrap();
    let split = Split::of(&entry).unwrap().unwrap();
    delete(&wide(&part_target(&scratch.service, main, split.set, 1))).unwrap();
    let error = read(&scratch.service, main).unwrap_err();
    assert!(
        matches!(&error, crate::Error::CredentialManager(Error::Read(source)) if source.kind() == io::ErrorKind::NotFound),
        "{error:?}"
    );
}

#[test]
fn a_secret_that_starts_like_a_split_entry_is_kept_as_given() {
    let scratch = Scratch::new("marker");
    let [main, _] = scratch.accounts;
    let secret = "herdr-parts:a:1";
    scratch.save(main, Some(secret)).unwrap();
    assert_eq!(scratch.read(main).as_deref(), Some(secret));
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
