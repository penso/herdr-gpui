use super::*;
use crate::push::vapid::{TOKEN_LIFETIME_S, key_path};
use p256::{
    PublicKey,
    ecdsa::{Signature, VerifyingKey, signature::Verifier as _},
};

const NOW: u64 = 1_800_000_000;

/// The parts of an `Authorization: vapid t=…,k=…` header.
fn split(header: &str) -> (&str, &str) {
    let rest = header.strip_prefix("vapid t=").unwrap();
    rest.split_once(",k=").unwrap()
}

#[test]
fn authorization_is_an_es256_token_the_public_key_verifies() {
    let dir = tempfile::tempdir().unwrap();
    let vapid = Vapid::load_or_create(dir.path()).unwrap();
    let header = vapid
        .authorization("https://fcm.googleapis.com", "mailto:me@example.com", NOW)
        .unwrap();
    let (token, key) = split(&header);
    assert_eq!(key, vapid.public_key_b64url());

    let mut parts = token.split('.');
    let (head, claims, signature) = (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    );
    assert!(parts.next().is_none());
    let head: serde_json::Value = serde_json::from_slice(&b64(head)).unwrap();
    assert_eq!(head["alg"], "ES256");
    assert_eq!(head["typ"], "JWT");
    let claims: serde_json::Value = serde_json::from_slice(&b64(claims)).unwrap();
    assert_eq!(claims["aud"], "https://fcm.googleapis.com");
    assert_eq!(claims["exp"], NOW + TOKEN_LIFETIME_S);
    assert_eq!(claims["sub"], "mailto:me@example.com");

    let public = PublicKey::from_sec1_bytes(&b64(key)).unwrap();
    let signature = Signature::from_slice(&b64(signature)).unwrap();
    let signing_input = token.rsplit_once('.').unwrap().0;
    VerifyingKey::from(public)
        .verify(signing_input.as_bytes(), &signature)
        .unwrap();
}

#[test]
fn public_key_is_an_uncompressed_point() {
    let dir = tempfile::tempdir().unwrap();
    let vapid = Vapid::load_or_create(dir.path()).unwrap();
    let point = b64(vapid.public_key_b64url());
    assert_eq!(point.len(), 65);
    assert_eq!(point[0], 0x04);
}

#[test]
fn subject_must_be_mailto_or_https() {
    let dir = tempfile::tempdir().unwrap();
    let vapid = Vapid::load_or_create(dir.path()).unwrap();
    for subject in ["", "mailto:", "http://example.com", "me@example.com"] {
        let error = vapid
            .authorization("https://fcm.googleapis.com", subject, NOW)
            .unwrap_err();
        assert!(matches!(error, Error::Subject), "{subject}");
    }
    assert!(
        vapid
            .authorization("https://fcm.googleapis.com", "https://example.com", NOW)
            .is_ok()
    );
    assert!(matches!(
        Sender::new(vapid, "nobody").unwrap_err(),
        Error::Subject
    ));
}

#[test]
fn key_is_created_once_and_reloaded() {
    let dir = tempfile::tempdir().unwrap();
    let first = Vapid::load_or_create(dir.path()).unwrap();
    let second = Vapid::load_or_create(dir.path()).unwrap();
    assert_eq!(first.public_key_b64url(), second.public_key_b64url());

    let path = key_path(dir.path());
    let pem = std::fs::read_to_string(&path).unwrap();
    assert!(pem.starts_with("-----BEGIN EC PRIVATE KEY-----"));
    // Only the key is left; the temporary file is gone.
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["vapid.pem"]);
}

#[test]
fn a_damaged_key_is_reported_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    Vapid::load_or_create(dir.path()).unwrap();
    let path = key_path(dir.path());
    std::fs::write(&path, "not a key").unwrap();
    let error = Vapid::load_or_create(dir.path()).unwrap_err();
    assert!(matches!(error, Error::KeyDecode { .. }), "{error:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a key");
}

#[cfg(unix)]
#[test]
fn key_and_directory_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    Vapid::load_or_create(dir.path()).unwrap();
    let path = key_path(dir.path());
    let mode =
        |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let error = Vapid::load_or_create(dir.path()).unwrap_err();
    assert!(matches!(error, Error::KeyNotPrivate { .. }), "{error:?}");
}
