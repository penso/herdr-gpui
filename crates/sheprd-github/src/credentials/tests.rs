#![allow(clippy::unwrap_used)]
use super::*;
use crate::token::Credential;
use rustix::process::geteuid;
use secrecy::ExposeSecret;
use sheprd_credentials::LIMIT;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

const NAME: &CStr = c"github-credentials";

#[test]
fn accounts_are_separate_files_and_removing_one_keeps_the_other() {
    let path = tempfile::tempdir().unwrap();
    std::fs::set_permissions(path.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let host = c"github-credentials-0123456789abcdef0123456789abcdef";
    let token = |access: &str| {
        Credential::new(access.into(), None, "client")
            .unwrap()
            .encode()
            .unwrap()
    };
    store(path.path(), NAME, Some(&token("main")), true).unwrap();
    store(path.path(), host, Some(&token("host")), true).unwrap();
    let access = |name| {
        Credential::decode(&read(path.path(), name).unwrap().unwrap())
            .unwrap()
            .access_token
            .expose_secret()
            .to_owned()
    };
    assert_eq!(access(NAME), "main");
    assert_eq!(access(host), "host");
    store(path.path(), host, None, false).unwrap();
    assert!(read(path.path(), host).unwrap().is_none());
    assert_eq!(access(NAME), "main");
}

#[test]
fn refresh_record_roundtrips_maximum_escaped_tokens_in_a_private_file() {
    let path = tempfile::tempdir().unwrap();
    std::fs::set_permissions(path.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let access = "\"\\".repeat(2048);
    let refresh = "\\\"".repeat(2048);
    let client = "c".repeat(256);
    let credential = Credential::new(
        access.as_str().into(),
        Some(refresh.as_str().into()),
        &client,
    )
    .unwrap();
    let encoded = credential.encode().unwrap();
    // Every token character needs JSON escaping; the issuing client is also maximal.
    assert!(encoded.expose_secret().len() > 2 * 4096 * 2);
    assert!(encoded.expose_secret().len() <= LIMIT);
    store(path.path(), NAME, Some(&encoded), true).unwrap();
    let file = path.path().join("github-credentials");
    let metadata = std::fs::metadata(&file).unwrap();
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert_eq!(metadata.uid(), geteuid().as_raw());
    assert_eq!(metadata.nlink(), 1);
    assert_eq!(metadata.len(), encoded.expose_secret().len() as u64);
    let saved = read(path.path(), NAME).unwrap().unwrap();
    assert_eq!(saved.expose_secret(), encoded.expose_secret());
    let restored = Credential::decode(&saved).unwrap();
    assert_eq!(restored.access_token.expose_secret(), access);
    assert_eq!(
        restored.refresh_token.as_ref().unwrap().expose_secret(),
        refresh
    );
    let record: serde_json::Value = serde_json::from_str(saved.expose_secret()).unwrap();
    assert_eq!(record["client_id"], client);
    assert_eq!(record["version"], 1);
    store(path.path(), NAME, None, false).unwrap();
    assert!(read(path.path(), NAME).unwrap().is_none());
    assert!(!file.exists());
}
