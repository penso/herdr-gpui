//! Renewing a connected GitLab account: GitLab is asked without holding the
//! saved record, a sign-in changed meanwhile is not brought back, and a pair
//! the store refuses is still used and saved later.
use crate::gitlab::saved::{Kept, OAuth, read_in, save_kept_in, token_in};
use herdr_credentials::Store;
use secrecy::{ExposeSecret, SecretString};
use std::path::Path;

fn oauth(access: &str, refresh: &str, expires_at: u64) -> OAuth {
    OAuth {
        access: SecretString::from(access),
        refresh: SecretString::from(refresh),
        expires_at,
        application: "app-id".into(),
    }
}

fn connect(path: &Path, host: &str, access: &str) {
    let kept = Kept::OAuth(oauth(access, "refresh-1", 1_000));
    save_kept_in(Store::File, path, host, Some(kept)).unwrap();
}

fn kept(path: &Path, host: &str) -> Option<String> {
    let saved = read_in(Store::File, path).unwrap();
    saved
        .get(host)
        .map(|token| token.expose_secret().to_owned())
}

#[test]
fn saves_go_on_while_gitlab_renews_a_sign_in() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    connect(path, "gitlab.com", "old");
    let token = token_in(Store::File, path, "gitlab.com", 1_000, |_, now| {
        // The record is free: this save would wait forever otherwise.
        save_kept_in(
            Store::File,
            path,
            "gitlab.example.com",
            Some(Kept::Token(SecretString::from("glpat-example"))),
        )
        .unwrap();
        Ok(oauth("new", "refresh-2", now + 7_200))
    })
    .unwrap();
    assert_eq!(token.unwrap().expose_secret(), "new");
    assert_eq!(kept(path, "gitlab.com").as_deref(), Some("new"));
    assert_eq!(
        kept(path, "gitlab.example.com").as_deref(),
        Some("glpat-example")
    );
}

#[test]
fn a_sign_in_disconnected_during_a_renewal_stays_disconnected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    connect(path, "gitlab.com", "old");
    token_in(Store::File, path, "gitlab.com", 1_000, |_, now| {
        save_kept_in(Store::File, path, "gitlab.com", None).unwrap();
        Ok(oauth("new", "refresh-2", now + 7_200))
    })
    .unwrap();
    assert_eq!(kept(path, "gitlab.com"), None);
}

#[test]
fn a_renewed_pair_the_store_refuses_is_used_and_saved_later() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    connect(path, "gitlab.com", "old");
    let lock = |mode| fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    let token = token_in(Store::File, path, "gitlab.com", 1_000, |_, now| {
        lock(0o500);
        Ok(oauth("new", "refresh-2", now + 7_200))
    })
    .unwrap();
    assert_eq!(token.unwrap().expose_secret(), "new");
    assert_eq!(kept(path, "gitlab.com").as_deref(), Some("old"));
    // GitLab spent the stored pair: the next lookup uses the renewed one.
    let token = token_in(Store::File, path, "gitlab.com", 1_100, |_, _| {
        panic!("renewed again")
    })
    .unwrap();
    assert_eq!(token.unwrap().expose_secret(), "new");
    lock(0o700);
    token_in(Store::File, path, "gitlab.com", 1_200, |_, _| {
        panic!("renewed again")
    })
    .unwrap();
    assert_eq!(kept(path, "gitlab.com").as_deref(), Some("new"));
}

/// A pair the store refused is renewed again when due. Another window's
/// lookup may save that refused pair while GitLab answers; the newly
/// granted pair still replaces it, since the store would otherwise keep a
/// refresh token GitLab already spent.
#[test]
fn a_refused_pair_saved_during_its_own_renewal_is_still_replaced() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    connect(path, "gitlab.com", "old");
    let lock = |mode| fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    lock(0o500);
    token_in(Store::File, path, "gitlab.com", 1_000, |_, now| {
        Ok(oauth("new", "refresh-2", now + 7_200))
    })
    .unwrap();
    assert_eq!(kept(path, "gitlab.com").as_deref(), Some("old"));
    // Due again, with the store still refusing the pending pair.
    let token = token_in(Store::File, path, "gitlab.com", 9_000, |renewing, now| {
        assert_eq!(renewing.refresh.expose_secret(), "refresh-2");
        lock(0o700);
        // What another window's lookup writes while GitLab answers.
        let pending = Kept::OAuth(oauth("new", "refresh-2", 8_200));
        save_kept_in(Store::File, path, "gitlab.com", Some(pending)).unwrap();
        Ok(oauth("newest", "refresh-3", now + 7_200))
    })
    .unwrap();
    assert_eq!(token.unwrap().expose_secret(), "newest");
    assert_eq!(kept(path, "gitlab.com").as_deref(), Some("newest"));
}
