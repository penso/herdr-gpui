use super::*;
use std::process::Child;
use std::time::Instant;

struct Agent(Child);

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn an_agent_backed_stale_public_half_is_refused_without_changing_either_file() {
    if !tools() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let directory = directory(root.path(), false);
    let selected = generate(&directory, "selected", &|| false).unwrap();
    let other = generate(&directory, "other", &|| false).unwrap();
    let public = public_path(&selected);
    fs::copy(public_path(&other), &public).unwrap();

    // Use a private agent, never the user's agent or environment. This is the
    // identity OpenSSH would select from the stale public half.
    let socket = root.path().join("agent");
    let agent = Command::new("ssh-agent")
        .args(["-D", "-a"])
        .arg(&socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let mut agent = match agent {
        Ok(child) => Agent(child),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => panic!("ssh-agent: {error}"),
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !socket.exists() {
        assert!(agent.0.try_wait().unwrap().is_none(), "ssh-agent exited");
        assert!(Instant::now() < deadline, "ssh-agent did not bind");
        std::thread::sleep(Duration::from_millis(10));
    }
    let add = |args: &[&std::ffi::OsStr]| {
        Command::new("ssh-add")
            .args(args)
            .env("SSH_AUTH_SOCK", &socket)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    assert!(add(&[other.as_os_str()]).status.success());
    assert!(add(&["-T".as_ref(), public.as_os_str()]).status.success());
    let before = (fs::read(&selected).unwrap(), fs::read(&public).unwrap());
    let error = fingerprint(&selected, &|| false).unwrap_err();
    assert!(matches!(&error, Error::PublicKeyMismatch(path) if *path == selected));
    assert!(error.to_string().contains("Restore the matching .pub file"));
    assert_eq!(
        before,
        (fs::read(selected).unwrap(), fs::read(public).unwrap())
    );
}

#[test]
fn matching_public_keys_ignore_comments_and_private_keys_work_without_a_public_half() {
    if !tools() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let key = generate(&directory(root.path(), false), "key", &|| false).unwrap();
    let expected = fingerprint(&key, &|| false).unwrap();
    let public = public_path(&key);
    let text = fs::read_to_string(&public).unwrap();
    let fields: Vec<_> = text.split_whitespace().take(2).collect();
    fs::write(
        &public,
        format!("{} {} another comment\n", fields[0], fields[1]),
    )
    .unwrap();
    assert_eq!(fingerprint(&key, &|| false).unwrap(), expected);
    fs::remove_file(public).unwrap();
    assert_eq!(fingerprint(&key, &|| false).unwrap(), expected);
}

#[test]
fn encrypted_keys_keep_their_public_identity_and_malformed_public_halves_are_refused() {
    if !tools() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let key = generate(&directory(root.path(), false), "key", &|| false).unwrap();
    let expected = fingerprint(&key, &|| false).unwrap();
    let public = public_path(&key);
    let original = fs::read(&public).unwrap();
    fs::write(&public, "not a public key\n").unwrap();
    assert!(matches!(
        fingerprint(&key, &|| false),
        Err(Error::PublicKeyMismatch(_))
    ));
    fs::write(&public, original).unwrap();
    let encrypted = Command::new("ssh-keygen")
        .args(["-p", "-P", "", "-N", "test-passphrase", "-f"])
        .arg(&key)
        .output()
        .unwrap();
    assert!(encrypted.status.success());
    assert_eq!(fingerprint(&key, &|| false).unwrap(), expected);
}
