use super::*;

/// A host account of this test run's own, so the user's real sign-ins are
/// never touched. Removed again when dropped.
struct Host(Account);

impl Drop for Host {
    fn drop(&mut self) {
        let _ = save(None, Store::Keyring, &self.0);
    }
}

#[test]
fn a_sign_in_saved_in_credential_manager_is_read_back_after_a_restart() {
    assert_eq!(
        select(&crate::config::GitHubConfig::default()),
        Store::Keyring
    );
    let id = format!("{:032x}", u128::from(std::process::id()) << 64 | 0x7e57);
    let host = Host(Account::host(&id).unwrap());
    assert!(saved_token(Store::Keyring, &host.0).unwrap().is_none());
    // A device-flow pair at GitHub's real token lengths.
    let saved = Credential::new(
        format!("ghu_{}", "a".repeat(36)).into(),
        Some(format!("ghr_{}", "b".repeat(76)).into()),
        "Iv23liurUcwxPjrdIFYT",
    )
    .unwrap()
    .encode()
    .unwrap();
    save(Some(&saved), Store::Keyring, &host.0).unwrap();
    // Nothing is cached in the process: this is what the next launch reads.
    let read = saved_token(Store::Keyring, &host.0).unwrap().unwrap();
    assert_eq!(read.expose_secret(), saved.expose_secret());
    assert!(matches!(Saved::decode(&read).unwrap(), Saved::Token(_)));
    save(None, Store::Keyring, &host.0).unwrap();
    assert!(saved_token(Store::Keyring, &host.0).unwrap().is_none());
    save(None, Store::Keyring, &host.0).unwrap();
}
