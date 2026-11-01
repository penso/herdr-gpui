use super::*;

#[test]
fn no_herdr_key_leaves_identities_to_openssh() {
    assert!(options_for(Path::new("/home/a/.ssh/herdr_ed25519"), |_| false).is_empty());
}

#[test]
fn herdr_key_comes_after_the_default_keys_present() {
    // Built as the code builds them: Windows joins with `\\`.
    let directory = Path::new("/home/a/.ssh");
    let (key, default) = (directory.join(KEY_NAME), directory.join("id_ed25519"));
    let options = options_for(&key, |path| path == key || path == default);
    let option = |path: &Path| format!("IdentityFile=\"{}\"", path.display());
    assert_eq!(
        options,
        [
            "-o".to_owned(),
            option(&default),
            "-o".to_owned(),
            option(&key)
        ]
    );
}

#[test]
fn percent_is_escaped_and_quotes_are_refused() {
    assert_eq!(
        identity_option(Path::new("/h/100%/k")).as_deref(),
        Some("IdentityFile=\"/h/100%%/k\"")
    );
    assert_eq!(identity_option(Path::new("/h/\"/k")), None);
}
