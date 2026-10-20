//! Runs the PowerShell scripts in this machine's PowerShell, as a Windows
//! OpenSSH host would after decoding `-EncodedCommand`. They drive the
//! PowerShell child directly, with no `ssh` in between.

use super::super::folder as scripts;
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

    fn probe(&self, path: &str) -> Run {
        run(
            self.shell,
            &scripts::powershell_probe_script(path),
            &[],
            self.home.path(),
        )
    }

    fn archive(&self, path: &str, max_bytes: u64) -> Run {
        run(
            self.shell,
            &scripts::powershell_archive_script(path, max_bytes),
            &[],
            self.home.path(),
        )
    }

    fn push(&self, path: &str, expect: Expect, input: &[u8]) -> Run {
        self.push_claiming(path, expect, input.len() as u64, input)
    }

    /// A push that says it sends `len` bytes and sends `input`.
    fn push_claiming(&self, path: &str, expect: Expect, len: u64, input: &[u8]) -> Run {
        run(
            self.shell,
            &powershell::push_script(path, expect, Access::Keep, len),
            input,
            self.home.path(),
        )
    }

    /// A push of a new file at `path` that takes the access of `source`.
    fn push_like(&self, path: &str, source: &str, input: &[u8]) -> Run {
        let script = powershell::push_script(
            path,
            Expect::Absent,
            Access::Like(source),
            input.len() as u64,
        );
        run(self.shell, &script, input, self.home.path())
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

    // A transfer cut off short of its length replaces nothing.
    for expect in [Expect::Sha256(sha256(&second)), Expect::Any] {
        let short = fixture.push_claiming(&path, expect, 5000, &first[..1000]);
        assert_code(&short, SHORT_EXIT);
        assert!(std::fs::read(&path).unwrap() == second);
        assert_eq!(fixture.files(), before);
    }
    let fresh = fixture.text("never.txt");
    assert_code(
        &fixture.push_claiming(&fresh, Expect::Absent, 9, b"new"),
        SHORT_EXIT,
    );
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

    // A kept-both copy takes its source's ACL; a source whose ACL cannot be
    // read is refused before the temporary file exists.
    let kept = fixture.text("kept.txt");
    assert_code(&fixture.push_like(&kept, &path, b"kept"), 0);
    assert_eq!(std::fs::read(&kept).unwrap(), b"kept");
    let gone = fixture.push_like(&fixture.text("gone.txt"), &fixture.text("missing"), b"x");
    assert_code(&gone, ACCESS_EXIT);
    // A folder has an ACL of its own, which is no file's to copy.
    let folder = fixture.push_like(&fixture.text("dir.txt"), &fixture.text("folder"), b"x");
    assert_code(&folder, ACCESS_EXIT);
    let mut expected = before.clone();
    expected.extend([
        "folder".to_owned(),
        format!("{HOSTILE_NAME}.new"),
        "kept.txt".to_owned(),
    ]);
    expected.sort();
    assert_eq!(fixture.files(), expected);
}

/// Runs `script` under Windows PowerShell and returns its trimmed stdout.
#[cfg(windows)]
fn windows_powershell(script: &str) -> String {
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
        ])
        .arg(encode(&format!(
            "$ErrorActionPreference = 'Stop'\n{script}"
        )))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{script}\nstderr: {stderr}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// Waits for the push's temporary file to appear in `folder`. The deadline
/// is a hang guard only; a script that exits first fails at once.
#[cfg(windows)]
fn wait_for_temporary(folder: &Path, prefix: &str, child: &mut std::process::Child) -> PathBuf {
    let deadline = std::time::Instant::now() + Duration::from_secs(300);
    loop {
        if let Some(name) = entries(folder)
            .into_iter()
            .find(|name| name.starts_with(prefix))
        {
            return folder.join(name);
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("the push exited before creating its temporary file: {status}");
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no temporary file appeared"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The access-control part of `path`'s security descriptor, as SDDL.
#[cfg(windows)]
fn sddl(path: &Path) -> String {
    windows_powershell(&format!(
        "[System.IO.File]::GetAccessControl('{}', 'Access').GetSecurityDescriptorSddlForm('Access')",
        path.to_str().unwrap()
    ))
}

/// A folder whose inheritable entries let Everyone read, holding
/// `secret.txt`, whose own protected ACL lets only this user in.
#[cfg(windows)]
fn guarded_secret() -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let folder = home.path().to_str().unwrap();
    let target = home.path().join("secret.txt");
    windows_powershell(&format!(
        r#"$acl = [System.IO.Directory]::GetAccessControl('{folder}')
$everyone = New-Object System.Security.Principal.SecurityIdentifier('S-1-1-0')
$acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($everyone, 'Read', 'ContainerInherit, ObjectInherit', 'None', 'Allow')))
[System.IO.Directory]::SetAccessControl('{folder}', $acl)"#
    ));
    std::fs::write(&target, b"old").unwrap();
    let target_text = target.to_str().unwrap();
    windows_powershell(&format!(
        r#"$acl = New-Object System.Security.AccessControl.FileSecurity
$acl.SetAccessRuleProtection($true, $false)
$me = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($me, 'FullControl', 'Allow')))
[System.IO.File]::SetAccessControl('{target_text}', $acl)"#
    ));
    assert!(!sddl(&target).contains(";WD)"), "the target is owner-only");
    (home, target)
}

/// A push to a file whose ACL is stricter than its folder's inheritable
/// entries must not let the folder's readers read the upload while it is
/// still in the temporary file, under either PowerShell.
#[cfg(windows)]
#[test]
#[ignore = "spawns this machine's powershell.exe and pwsh"]
fn powershell_push_temporary_file_takes_the_targets_acl() {
    for shell in shells() {
        eprintln!("running under {shell}");
        let (home, target) = guarded_secret();
        let script =
            powershell::push_script(target.to_str().unwrap(), Expect::Any, Access::Keep, 10);
        let mut child = Command::new(shell)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
            ])
            .arg(encode(&script))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all(b"new").unwrap();
        input.flush().unwrap();
        let temporary = wait_for_temporary(home.path(), ".secret.txt.", &mut child);
        // Reading an ACL needs only READ_CONTROL, which `FileShare.None`
        // allows.
        let temporary_sddl = sddl(&temporary);
        drop(input);
        let output = child.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(SHORT_EXIT), "stderr: {stderr}");
        assert!(
            !temporary_sddl.contains(";WD)"),
            "{shell}: the folder's Everyone entry reached the temporary file: {temporary_sddl}"
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert_eq!(entries(home.path()), ["secret.txt"]);
    }
}

/// A kept-both copy of an owner-only file must not take the folder's wider
/// inheritable entries, under either PowerShell.
#[cfg(windows)]
#[test]
#[ignore = "spawns this machine's powershell.exe and pwsh"]
fn powershell_push_kept_copy_takes_its_sources_acl() {
    for shell in shells() {
        eprintln!("running under {shell}");
        let (home, target) = guarded_secret();
        let fixture = Fixture { shell, home };
        let copy = fixture.text("secret.conflict.txt");
        let pushed = fixture.push_like(&copy, target.to_str().unwrap(), b"mine");
        assert_code(&pushed, 0);
        assert_eq!(std::fs::read(&copy).unwrap(), b"mine");
        let copy_sddl = sddl(Path::new(&copy));
        assert!(
            !copy_sddl.contains(";WD)"),
            "{shell}: the folder's Everyone entry reached the kept copy: {copy_sddl}"
        );
    }
}

fn probes(fixture: &Fixture) {
    let tree = "a tree";
    std::fs::create_dir_all(fixture.path(&format!("{tree}/nested"))).unwrap();
    std::fs::write(fixture.path(&format!("{tree}/a.txt")), [b'a'; 100]).unwrap();
    std::fs::write(fixture.path(&format!("{tree}/nested/b.bin")), payload(5000)).unwrap();

    let first = fixture.probe(&fixture.text(tree));
    assert_code(&first, 0);
    let first = parse_probe_output(&first.stdout).unwrap();
    assert_eq!(first.bytes, 5 * 1024, "100 + 5000 bytes rounds up to 5 KiB");
    assert_eq!(first.top, tree);
    let fingerprint = first.fingerprint.expect("PowerShell always has SHA-256");

    let trailing = fixture.probe(&format!(
        "{}{}",
        fixture.text(tree),
        std::path::MAIN_SEPARATOR
    ));
    assert_code(&trailing, 0);
    assert_eq!(parse_probe_output(&trailing.stdout).unwrap(), first);

    let home = fixture.probe(&format!("~\\{tree}"));
    assert_code(&home, 0);
    assert_eq!(parse_probe_output(&home.stdout).unwrap(), first);
    let home = fixture.probe(&format!("~/{tree}"));
    assert_code(&home, 0);
    assert_eq!(parse_probe_output(&home.stdout).unwrap(), first);

    let again = fixture.probe(&fixture.text(tree));
    assert_code(&again, 0);
    assert_eq!(parse_probe_output(&again.stdout).unwrap(), first);

    std::fs::create_dir(fixture.path(&format!("{tree}/empty"))).unwrap();
    let empty = fixture.probe(&fixture.text(tree));
    assert_code(&empty, 0);
    assert_ne!(
        parse_probe_output(&empty.stdout).unwrap().fingerprint,
        Some(fingerprint),
        "an empty folder changes the fingerprint"
    );

    std::fs::write(fixture.path(&format!("{tree}/added.txt")), b"added").unwrap();
    let changed = fixture.probe(&fixture.text(tree));
    assert_code(&changed, 0);
    assert_ne!(
        parse_probe_output(&changed.stdout).unwrap().fingerprint,
        Some(fingerprint)
    );

    assert_code(&fixture.probe(&fixture.text("missing")), MISSING_EXIT);
    std::fs::write(fixture.path("file"), b"x").unwrap();
    assert_code(&fixture.probe(&fixture.text("file")), NOT_A_FILE_EXIT);
}

fn archives(fixture: &Fixture) {
    let tree = "it's a tree";
    std::fs::create_dir_all(fixture.path(&format!("{tree}/nested"))).unwrap();
    std::fs::create_dir(fixture.path(&format!("{tree}/empty"))).unwrap();
    std::fs::write(fixture.path(&format!("{tree}/a.txt")), b"alpha").unwrap();
    std::fs::write(fixture.path(&format!("{tree}/nested/b.bin")), payload(4096)).unwrap();

    let probed = fixture.probe(&fixture.text(tree));
    assert_code(&probed, 0);
    let top = parse_probe_output(&probed.stdout).unwrap().top;

    let zipped = fixture.archive(&fixture.text(tree), MAX_REMOTE_FILE_BYTES);
    assert_code(&zipped, 0);
    assert!(zipped.stdout.starts_with(b"PK"), "not a zip");

    let zip = fixture.path("archive.zip");
    std::fs::write(&zip, &zipped.stdout).unwrap();
    let listing = format!(
        "Add-Type -AssemblyName System.IO.Compression.FileSystem\n\
         $zip = [System.IO.Compression.ZipFile]::OpenRead({})\n\
         try {{ $zip.Entries | ForEach-Object {{ $_.FullName }} }} finally {{ $zip.Dispose() }}",
        powershell::literal(zip.to_str().unwrap())
    );
    let listed = run(fixture.shell, &listing, &[], fixture.home.path());
    assert_code(&listed, 0);
    let mut names = String::from_utf8(listed.stdout)
        .unwrap()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    names.sort();
    assert!(names.contains(&format!("{tree}/empty/")), "{names:?}");
    for name in &names {
        assert_eq!(name.split('/').next(), Some(top.as_str()), "{name}");
    }
    assert_eq!(
        names,
        vec![
            format!("{tree}/a.txt"),
            format!("{tree}/empty/"),
            format!("{tree}/nested/"),
            format!("{tree}/nested/b.bin"),
        ]
    );

    let too_large = fixture.archive(&fixture.text(tree), 64);
    assert_code(&too_large, TOO_LARGE_EXIT);
    assert!(too_large.stdout.is_empty());

    assert_code(
        &fixture.archive(&fixture.text("missing"), MAX_REMOTE_FILE_BYTES),
        MISSING_EXIT,
    );
}

/// A reparse point inside the tree is skipped, so a junction that points at
/// its own parent neither recurses nor changes the size or fingerprint, and
/// the probe counts it as left out.
#[cfg(windows)]
#[test]
fn powershell_probe_skips_junctions() {
    let shells = shells();
    if shells.is_empty() {
        eprintln!("skipped: no PowerShell on this machine");
        return;
    }
    for shell in shells {
        eprintln!("running under {shell}");
        let fixture = Fixture {
            shell,
            home: tempfile::tempdir().unwrap(),
        };
        let tree = "tree";
        std::fs::create_dir(fixture.path(tree)).unwrap();
        std::fs::write(fixture.path("tree/a.txt"), b"alpha").unwrap();
        let before = fixture.probe(&fixture.text(tree));
        assert_code(&before, 0);
        let before = parse_probe_output(&before.stdout).unwrap();

        let created = run(
            shell,
            &format!(
                "New-Item -ItemType Junction -Path {} -Target {} | Out-Null",
                powershell::literal(&fixture.text(&format!("{tree}/link"))),
                powershell::literal(&fixture.text(tree)),
            ),
            &[],
            fixture.home.path(),
        );
        assert_code(&created, 0);

        let after = fixture.probe(&fixture.text(tree));
        assert_code(&after, 0);
        let after = parse_probe_output(&after.stdout).unwrap();
        assert_eq!(after.bytes, before.bytes, "the junction adds no size");
        assert_eq!(
            after.fingerprint, before.fingerprint,
            "the junction changes no fingerprint"
        );
        assert_eq!(
            after.links_skipped, 1,
            "the junction is counted as left out"
        );
    }
}

#[test]
fn powershell_scripts_probe_and_archive_folders() {
    let shells = shells();
    if shells.is_empty() {
        eprintln!("skipped: no PowerShell on this machine");
        return;
    }
    for shell in shells {
        eprintln!("running under {shell}");
        let probe = Fixture {
            shell,
            home: tempfile::tempdir().unwrap(),
        };
        probes(&probe);
        let archive = Fixture {
            shell,
            home: tempfile::tempdir().unwrap(),
        };
        archives(&archive);
    }
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
