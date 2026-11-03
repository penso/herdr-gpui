#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn a_token_is_made_once_and_kept() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state").join(FILE);
    let made = Token::load_or_create(&path).unwrap();
    assert_eq!(made.as_str().len(), BYTES * 2);
    assert!(made.as_str().bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(fs::read_to_string(&path).unwrap(), made.as_str());
    assert_eq!(
        Token::load_or_create(&path).unwrap().as_str(),
        made.as_str()
    );
    // Each new file gets its own.
    let other = Token::load_or_create(&temp.path().join("other")).unwrap();
    assert_ne!(other.as_str(), made.as_str());
}

#[cfg(unix)]
#[test]
fn only_the_owner_can_read_the_token_file() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(FILE);
    Token::load_or_create(&path).unwrap();
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600);
    // A file left readable by others is closed to them, keeping its token.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let kept = fs::read_to_string(&path).unwrap();
    assert_eq!(Token::load_or_create(&path).unwrap().as_str(), kept);
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn a_hand_written_token_is_used_and_anything_else_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(FILE);
    fs::write(&path, "my-own_Token1\n").unwrap();
    assert_eq!(
        Token::load_or_create(&path).unwrap().as_str(),
        "my-own_Token1"
    );
    for invalid in [
        "".to_owned(),
        "has space".into(),
        "?tkn=x".into(),
        "x".repeat(MAX_LEN + 1),
    ] {
        fs::write(&path, &invalid).unwrap();
        let token = Token::load_or_create(&path).unwrap();
        assert_eq!(token.as_str().len(), BYTES * 2, "{invalid}");
        assert_eq!(fs::read_to_string(&path).unwrap(), token.as_str());
    }
    fs::write(&path, [0xff, 0xfe]).unwrap();
    assert_eq!(
        Token::load_or_create(&path).unwrap().as_str().len(),
        BYTES * 2
    );
}

#[test]
fn the_token_is_never_printed() {
    let temp = tempfile::tempdir().unwrap();
    let token = Token::load_or_create(&temp.path().join(FILE)).unwrap();
    assert_eq!(format!("{token:?}"), "Token(..)");
    // Failures name the file, which holds no secret, not what is in it.
    let blocked = temp.path().join("file");
    fs::write(&blocked, token.as_str()).unwrap();
    let error = Token::load_or_create(&blocked.join(FILE)).unwrap_err();
    assert!(matches!(error, Error::TokenFile { .. }), "{error:?}");
    let message = format!("{error} {error:?}");
    assert!(message.contains(FILE));
    assert!(!message.contains(token.as_str()));
}

/// App instances starting together all get the one token that ends up in
/// the file, even when what was there held none.
#[test]
fn instances_making_the_token_together_share_one() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(FILE);
    for before in [None, Some("has space")] {
        if let Some(text) = before {
            fs::write(&path, text).unwrap();
        } else {
            let _ = fs::remove_file(&path);
        }
        let start = std::sync::Arc::new(std::sync::Barrier::new(8));
        let tokens: Vec<String> = (0..8)
            .map(|_| {
                let (start, path) = (start.clone(), path.clone());
                std::thread::spawn(move || {
                    start.wait();
                    Token::load_or_create(&path).unwrap().as_str().to_owned()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        let kept = fs::read_to_string(&path).unwrap();
        assert!(tokens.iter().all(|token| *token == kept), "{before:?}");
    }
}
