use super::*;

#[test]
fn additional_accounts_accept_only_config_ids_and_use_their_own_entries() {
    let id = "0123456789abcdef0123456789abcdef";
    let account = Account::additional(id).unwrap();
    for bad in ["", "../github-credentials", &id.to_uppercase(), &id[1..]] {
        assert_eq!(Account::additional(bad), None, "{bad}");
    }
    assert_eq!(
        account.file_name().to_str().unwrap(),
        format!("github-credentials-account-{id}")
    );
    // A device and an additional account with the same ID never share one.
    assert_ne!(account.file_name(), Account::host(id).unwrap().file_name());
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    assert_eq!(
        account.keyring_account(),
        format!("github.com/account/{id}")
    );
}

#[test]
fn an_additional_account_never_reads_the_environment_or_the_github_cli() {
    let account = Account::additional("0123456789abcdef0123456789abcdef").unwrap();
    // No store and no session choice: nothing to restore, whatever GH_TOKEN says.
    assert!(
        load_profile(Store::Environment, &account, true)
            .unwrap()
            .is_none()
    );
}
