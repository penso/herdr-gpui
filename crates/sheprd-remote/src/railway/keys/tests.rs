#![allow(clippy::unwrap_used)]
use super::*;
use crate::railway::tests::tools;

mod public_identity;

/// A key directory in Herdr's own folder, or in a `~/.ssh` under `root`.
fn directory(root: &Path, user: bool) -> KeyDirectory {
    if user {
        fs::create_dir(root.join(".ssh")).unwrap();
    }
    key_directory(user.then_some(root), root)
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn names_and_fingerprints_are_validated_strictly() {
    let id = id_from("256 SHA256:AbC+/dEfGhIjKlMnOpQrStUvWxYz0123456789abcde herdr (ED25519)\n");
    assert_eq!(id.as_deref(), Some("key-AbCdEfGhIjKlMnOp"));
    for bad in [
        "",
        "256",
        "256 MD5:aa:bb",
        "256 SHA256:+/+/ x",
        "256 SHA256:short x",
    ] {
        assert_eq!(id_from(bad), None, "{bad}");
    }
    for good in ["herdr-railway", "a", "Key_1.work", &"k".repeat(64)] {
        assert!(valid_name(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        ".hidden",
        "-option",
        "a/b",
        "../escape",
        "..",
        "a\\b",
        "name.pub",
        "with space",
        "nul\0",
        "é",
        &"k".repeat(65),
    ] {
        assert!(matches!(valid_name(bad), Err(Error::KeyName)), "{bad:?}");
    }
    // A bad name is refused before the disk is touched, even the folder.
    let root = tempfile::tempdir().unwrap();
    let directory = directory(root.path(), false);
    assert!(matches!(
        generate(&directory, "../escape", &|| false),
        Err(Error::KeyName)
    ));
    assert!(!directory.path.exists());
}

#[test]
fn new_keys_go_to_dot_ssh_only_when_it_is_a_directory() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let fallback = key_directory(Some(root.path()), &state);
    assert_eq!(fallback.path, state.join(KEYS));
    assert!(!fallback.user && !key_directory(None, &state).user);
    fs::write(root.path().join(".ssh"), "a file").unwrap();
    assert!(!key_directory(Some(root.path()), &state).user);
    fs::remove_file(root.path().join(".ssh")).unwrap();
    fs::create_dir(root.path().join(".ssh")).unwrap();
    let ssh = key_directory(Some(root.path()), &state);
    assert_eq!((ssh.path, ssh.user), (root.path().join(".ssh"), true));
}

#[test]
fn a_named_key_is_private_and_leaves_no_scratch() {
    if !tools() {
        return;
    }
    for user in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let directory = directory(root.path(), user);
        let path = generate(&directory, "herdr-railway", &|| false).unwrap();
        assert_eq!(path, directory.path.join("herdr-railway"));
        assert!(fingerprint(&path, &|| false).unwrap().starts_with("key-"));
        assert!(fs::read_to_string(&path).unwrap().contains("PRIVATE KEY"));
        assert!(
            !fs::read_to_string(public_path(&path))
                .unwrap()
                .contains("PRIVATE")
        );
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&path), 0o600);
            if !user {
                assert_eq!(mode(&directory.path), 0o700);
            }
        }
        assert_eq!(
            names(&directory.path),
            ["herdr-railway", "herdr-railway.pub"]
        );
    }
}

#[test]
fn generating_never_replaces_or_removes_what_is_already_there() {
    if !tools() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let directory = directory(root.path(), true);
    let target = directory.path.join("taken");
    let public = public_path(&target);
    let victim = root.path().join("victim");
    // Each existing name, a dangling symlink included, stops the run and
    // survives untouched; nothing is written through the link.
    for setup in 0..3 {
        let _ = fs::remove_file(&target);
        let _ = fs::remove_file(&public);
        match setup {
            0 => fs::write(&target, "mine").unwrap(),
            1 => fs::write(&public, "mine").unwrap(),
            _ => std::os::unix::fs::symlink(&victim, &target).unwrap(),
        }
        let before = names(&directory.path);
        let result = generate(&directory, "taken", &|| false);
        assert!(matches!(&result, Err(Error::KeyExists(path)) if *path == target));
        assert_eq!(names(&directory.path), before, "nothing added or removed");
        assert!(!victim.exists());
    }
    // A name taken after the up-front check is caught by the link itself.
    let (from, to) = (root.path().join("from"), root.path().join("to"));
    fs::write(&from, "new").unwrap();
    fs::write(&to, "mine").unwrap();
    assert!(matches!(place(&from, &to), Err(Error::KeyExists(path)) if path == to));
    assert_eq!(fs::read_to_string(&to).unwrap(), "mine");
    // Files that merely look like scratch names are not ours to clean up.
    let _ = fs::remove_file(&target);
    let _ = fs::remove_file(&public);
    let bystander = directory.path.join(".herdr-new-00000000");
    fs::write(&bystander, "not ours").unwrap();
    generate(&directory, "fresh", &|| false).unwrap();
    assert_eq!(fs::read_to_string(&bystander).unwrap(), "not ours");
}

#[test]
fn cancelling_kills_the_key_tool() {
    let mut hung = Command::new("/bin/sh");
    hung.args(["-c", "exec sleep 30"]);
    assert!(matches!(
        run(hung, "", &|| true),
        Err(Error::Cloud(cloud::Error::Cancelled))
    ));
}

#[test]
fn existing_keys_are_only_read_and_unusable_choices_fail_early() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(existing(Path::new("id_rsa")), Err(Error::KeyPath)));
    assert!(matches!(existing(dir.path()), Err(Error::KeyPath)));
    // A path the device list could not save is refused before any VM exists.
    assert!(matches!(
        existing(&dir.path().join("x".repeat(2048))),
        Err(Error::KeyPath)
    ));
    assert!(matches!(
        existing(&dir.path().join("absent")),
        Err(Error::KeyFile { .. })
    ));
    if !tools() {
        return;
    }
    let junk = dir.path().join("junk");
    fs::write(&junk, "this is not a key\n").unwrap();
    assert!(matches!(
        fingerprint(&junk, &|| false),
        Err(Error::UnusableKey(path)) if path == junk
    ));
    let path = generate(&directory(dir.path(), false), "mine", &|| false).unwrap();
    let before = fs::read(&path).unwrap();
    assert_eq!(existing(&path).unwrap(), path);
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn a_replaced_private_key_is_not_masked_by_its_old_public_half() {
    if !tools() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let directory = directory(root.path(), false);
    let first = generate(&directory, "first", &|| false).unwrap();
    let second = generate(&directory, "second", &|| false).unwrap();
    let (before, other) = (
        fingerprint(&first, &|| false).unwrap(),
        fingerprint(&second, &|| false).unwrap(),
    );
    // Swap in another private key while `first.pub` stays behind.
    fs::copy(&second, &first).unwrap();
    assert!(matches!(
        fingerprint(&first, &|| false),
        Err(Error::PublicKeyMismatch(path)) if path == first
    ));
    fs::remove_file(public_path(&first)).unwrap();
    let after = fingerprint(&first, &|| false).unwrap();
    assert_eq!(after, other);
    assert_ne!(after, before);
}
