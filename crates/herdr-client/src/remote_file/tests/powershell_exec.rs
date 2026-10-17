//! Runs the PowerShell scripts in this machine's PowerShell, as a Windows
//! OpenSSH host would after decoding `-EncodedCommand`. `run_script_command`
//! does not run on Windows, so these drive the child directly.

use super::*;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

/// Each PowerShell on this machine that starts.
fn shells() -> Vec<&'static str> {
    let candidates: &[&'static str] = if cfg!(windows) {
        &["powershell.exe", "pwsh"]
    } else {
        &["pwsh"]
    };
    candidates
        .iter()
        .copied()
        .filter(|shell| {
            Command::new(shell)
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "exit 0",
                ])
                .stdin(Stdio::null())
                .output()
                .is_ok_and(|output| output.status.success())
        })
        .collect()
}

struct Run {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: String,
}

/// Runs `script` with `stdin` on its stdin and `home` as its profile folder.
fn run(shell: &str, script: &str, stdin: &[u8], home: &Path) -> Run {
    let mut child = Command::new(shell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
        ])
        .arg(encode(script))
        .env("USERPROFILE", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    let mut errors = child.stderr.take().unwrap();
    let (stdout, stderr) = std::thread::scope(|scope| {
        // A script that refuses exits without reading its input; its exit
        // code is what the test checks.
        scope.spawn(move || drop(input.write_all(stdin)));
        let stderr = scope.spawn(move || {
            let mut text = Vec::new();
            errors.read_to_end(&mut text).unwrap();
            String::from_utf8_lossy(&text).into_owned()
        });
        let mut stdout = Vec::new();
        output.read_to_end(&mut stdout).unwrap();
        (stdout, stderr.join().unwrap())
    });
    let code = child.wait().unwrap().code();
    Run {
        code,
        stdout,
        stderr,
    }
}

#[track_caller]
fn assert_code(run: &Run, code: i32) {
    assert_eq!(run.code, Some(code), "stderr: {}", run.stderr);
}

struct Fixture {
    shell: &'static str,
    home: tempfile::TempDir,
}

impl Fixture {
    fn path(&self, name: &str) -> PathBuf {
        self.home.path().join(name)
    }

    fn text(&self, name: &str) -> String {
        self.path(name).to_str().unwrap().to_owned()
    }

    /// The profile folder's names, less the `AppData` PowerShell creates in
    /// any profile it starts with.
    fn files(&self) -> Vec<String> {
        let mut names = entries(self.home.path());
        names.retain(|name| name != "AppData");
        names
    }

    fn fetch(&self, path: &str, max_bytes: u64) -> Run {
        run(
            self.shell,
            &powershell::fetch_script(path, max_bytes),
            &[],
            self.home.path(),
        )
    }

    fn push(&self, path: &str, expect: Expect, input: &[u8]) -> Run {
        run(
            self.shell,
            &powershell::push_script(path, expect),
            input,
            self.home.path(),
        )
    }
}

fn fetches(fixture: &Fixture) {
    let original = payload(3 * 1024 * 1024 + 17);
    std::fs::write(fixture.path(HOSTILE_NAME), &original).unwrap();
    let fetched = fixture.fetch(&fixture.text(HOSTILE_NAME), MAX_REMOTE_FILE_BYTES);
    assert_code(&fetched, 0);
    assert!(fetched.stdout == original, "fetched bytes differ");

    let home = fixture.fetch(&format!("~\\{HOSTILE_NAME}"), MAX_REMOTE_FILE_BYTES);
    assert_code(&home, 0);
    assert!(home.stdout == original, "home-relative bytes differ");
    assert_code(
        &fixture.fetch(&format!("~/{HOSTILE_NAME}"), MAX_REMOTE_FILE_BYTES),
        0,
    );

    let too_large = fixture.fetch(&fixture.text(HOSTILE_NAME), original.len() as u64 - 1);
    assert_code(&too_large, TOO_LARGE_EXIT);
    assert!(too_large.stdout.is_empty());
    assert_code(&fixture.fetch(&fixture.text("missing"), 10), MISSING_EXIT);
    std::fs::create_dir(fixture.path("folder")).unwrap();
    assert_code(&fixture.fetch(&fixture.text("folder"), 10), NOT_A_FILE_EXIT);
}

fn pushes(fixture: &Fixture) {
    let name = "it's a file.txt";
    let path = fixture.text(name);
    let first = payload(4096);
    std::fs::write(&path, &first).unwrap();
    let before = fixture.files();

    let second = payload(5000).into_iter().rev().collect::<Vec<_>>();
    assert_code(
        &fixture.push(&path, Expect::Sha256(sha256(&first)), &second),
        0,
    );
    assert!(std::fs::read(&path).unwrap() == second);
    assert_eq!(fixture.files(), before);

    let stale = fixture.push(&path, Expect::Sha256(sha256(&first)), b"stale");
    assert_code(&stale, CONFLICT_EXIT);
    assert!(std::fs::read(&path).unwrap() == second);
    assert_eq!(fixture.files(), before);

    assert_code(
        &fixture.push(&path, Expect::Absent, b"absent"),
        CONFLICT_EXIT,
    );
    assert!(std::fs::read(&path).unwrap() == second);
    assert_eq!(fixture.files(), before);

    assert_code(&fixture.push(&path, Expect::Any, b"any"), 0);
    assert_eq!(std::fs::read(&path).unwrap(), b"any");

    let fresh = format!("~\\{HOSTILE_NAME}.new");
    assert_code(&fixture.push(&fresh, Expect::Absent, &first), 0);
    assert!(std::fs::read(fixture.path(&format!("{HOSTILE_NAME}.new"))).unwrap() == first);

    std::fs::create_dir(fixture.path("folder")).unwrap();
    assert_code(
        &fixture.push(&fixture.text("folder"), Expect::Any, b"x"),
        NOT_A_FILE_EXIT,
    );
    let missing = fixture.push(&fixture.text("missing"), Expect::Sha256(sha256(b"")), b"x");
    assert_code(&missing, CONFLICT_EXIT);
    let mut expected = before.clone();
    expected.extend(["folder".to_owned(), format!("{HOSTILE_NAME}.new")]);
    expected.sort();
    assert_eq!(fixture.files(), expected);
}

#[test]
#[ignore = "spawns this machine's powershell.exe and pwsh"]
fn powershell_scripts_fetch_and_push_files() {
    let shells = shells();
    if shells.is_empty() {
        eprintln!("skipped: no PowerShell on this machine");
        return;
    }
    for shell in shells {
        eprintln!("running under {shell}");
        let fetch = Fixture {
            shell,
            home: tempfile::tempdir().unwrap(),
        };
        fetches(&fetch);
        let push = Fixture {
            shell,
            home: tempfile::tempdir().unwrap(),
        };
        pushes(&push);
    }
}
