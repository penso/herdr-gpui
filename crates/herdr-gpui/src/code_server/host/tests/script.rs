//! `serve.sh` itself, run by this computer's `/bin/sh` against a stand-in
//! for VS Code's command line, in a private home folder.
#![cfg(unix)]
use super::*;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Stdio},
    time::Duration,
};

/// A stand-in `code`: it says it listens, keeps its arguments and the
/// token it was given, and leaves a child of its own running, as VS Code
/// leaves its server.
const FAKE_CODE: &str = r#"#!/bin/sh
printf '%s\n' "$@" > "$HOME/args"
cat "$5" > "$HOME/token-seen"
sleep 600 &
echo $! > "$HOME/grandchild"
echo "Web UI available on $3"
wait
"#;

fn home(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hvs-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(home: &Path, path: &str) -> Child {
    let socket = home.join("s.sock");
    let script = format!(
        "socket={}\n{SCRIPT}",
        herdr_client::shell_quote(&socket.to_string_lossy())
    );
    #[allow(clippy::disallowed_methods)]
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", &script])
        .env_clear()
        .env("HOME", home)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command.spawn().unwrap()
}

fn alive(pid: &str) -> bool {
    #[allow(clippy::disallowed_methods)]
    Command::new("kill")
        .args(["-0", pid.trim()])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn wait_for(mut done: impl FnMut() -> bool) -> bool {
    for _ in 0..500 {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn the_script_serves_with_the_token_and_stops_its_whole_tree() {
    let home = home("serve");
    let dir = home.join(".herdr-gpui/vscode");
    fs::create_dir_all(&dir).unwrap();
    let code = dir.join("code");
    fs::write(&code, FAKE_CODE).unwrap();
    fs::set_permissions(&code, fs::Permissions::from_mode(0o700)).unwrap();

    let mut child = run(&home, "/usr/bin:/bin");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"secret123\n")
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(line.starts_with("Web UI available on "), "{line}");
    assert!(line.trim_end().ends_with("s.sock"), "{line}");

    // The token reached VS Code through a private file, never an argument.
    assert!(wait_for(|| home.join("grandchild").exists()));
    let args = fs::read_to_string(home.join("args")).unwrap();
    assert!(args.starts_with("serve-web\n--socket-path\n"), "{args}");
    assert!(args.contains("--accept-server-license-terms"), "{args}");
    assert!(!args.contains("secret123"), "{args}");
    assert_eq!(
        fs::read_to_string(home.join("token-seen")).unwrap(),
        "secret123"
    );
    let token_file = dir.join("s.sock.token");
    let mode = fs::metadata(&token_file).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    // A host with no machine settings follows the page's color scheme.
    let settings = super::super::super::machine::path(&home);
    assert_eq!(
        fs::read_to_string(settings).unwrap(),
        super::super::super::machine::SETTINGS
    );

    // The app going away ends the input, which stops VS Code and the
    // server it started, and removes the token.
    let grandchild = fs::read_to_string(home.join("grandchild")).unwrap();
    assert!(alive(&grandchild));
    drop(child.stdin.take());
    assert!(wait_for(|| matches!(child.try_wait(), Ok(Some(_)))));
    assert!(
        wait_for(|| !alive(&grandchild)),
        "the server was left running"
    );
    assert!(!token_file.exists());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_host_without_a_downloader_says_so() {
    let home = home("nofetch");
    // The tools the script needs before it downloads, and no curl or wget.
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for tool in ["uname", "mkdir", "basename", "rm", "cat"] {
        let found = ["/usr/bin", "/bin"]
            .iter()
            .map(|dir| Path::new(dir).join(tool))
            .find(|path| path.exists())
            .unwrap();
        std::os::unix::fs::symlink(found, bin.join(tool)).unwrap();
    }
    let mut child = run(&home, &bin.to_string_lossy());
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"secret\n")
        .unwrap();
    let status = child.wait().unwrap();
    assert!(
        matches!(
            exited(status),
            super::super::super::Error::Host(Error::NoDownloader)
        ),
        "{status:?}"
    );
    fs::remove_dir_all(home).unwrap();
}
