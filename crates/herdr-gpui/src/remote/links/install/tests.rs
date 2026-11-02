use super::*;
mod parallel;

fn platform(os: &'static str, arch: &'static str) -> Platform {
    Platform { os, arch }
}

fn other_arch() -> &'static str {
    if Platform::local().arch == "aarch64" {
        "x86_64"
    } else {
        "aarch64"
    }
}

#[test]
fn a_host_like_this_computer_gets_this_apps_own_build() {
    let own = PathBuf::from("/app/herdr-remote-control");
    for release in [false, true] {
        // A Linux release's own build links glibc; the static one is sent
        // instead (`a_linux_host_of_a_release_gets_the_static_build_...`).
        if release && Platform::local().os == "linux" {
            continue;
        }
        assert_eq!(
            choose(&Platform::local(), release, Some(own.clone()), false),
            Some(Choice::Bundled(own.clone()))
        );
    }
}

#[test]
fn a_linux_host_of_a_release_gets_the_static_build_for_its_architecture() {
    // Even a host just like this computer, which may lack the glibc this
    // app's own build links.
    for arch in ["x86_64", "aarch64"] {
        let linux = platform("linux", arch);
        assert_eq!(
            choose(&linux, true, Some("/app/own".into()), false),
            Some(Choice::Download(format!("{arch}-unknown-linux-musl")))
        );
    }
}

#[test]
fn a_development_build_builds_from_source_for_another_platform() {
    let linux = platform("linux", other_arch());
    assert_eq!(
        choose(&linux, false, Some("/app/own".into()), false),
        Some(Choice::Source)
    );
    // Without its own build there is nothing else to copy either.
    let mac = platform("macos", other_arch());
    if mac != Platform::local() {
        assert_eq!(choose(&mac, false, None, false), Some(Choice::Source));
    }
}

#[test]
#[cfg(target_os = "macos")]
fn a_universal_build_serves_a_mac_of_either_architecture() {
    let other = platform("macos", other_arch());
    let own = PathBuf::from("/app/herdr-remote-control");
    assert_eq!(
        choose(&other, true, Some(own.clone()), true),
        Some(Choice::Bundled(own.clone()))
    );
    // A single-architecture release build has nothing for the other Mac.
    assert_eq!(choose(&other, true, Some(own), false), None);
}

#[test]
fn only_a_fat_mach_o_reads_as_universal() {
    let dir = tempfile::tempdir().unwrap();
    let fat = dir.path().join("fat");
    std::fs::write(&fat, [0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2]).unwrap();
    let thin = dir.path().join("thin");
    std::fs::write(&thin, [0xcf, 0xfa, 0xed, 0xfe, 7, 0, 0, 1]).unwrap();
    assert!(is_universal(&fat));
    assert!(!is_universal(&thin));
    assert!(!is_universal(&dir.path().join("missing")));
}

#[test]
fn the_build_script_stays_in_home_and_explains_a_missing_rust() {
    let script = build_script("dev-0123456789ab");
    assert!(script.contains(&format!("version={}", shell_quote("dev-0123456789ab"))));
    assert!(script.contains("cargo build --release --locked -p herdr-remote-control"));
    assert!(script.contains(&format!("exit {NO_RUST_EXIT}")));
    assert!(script.contains(
        "ln -sfn \"$dir/herdr-remote-control\" \"$HOME/.local/bin/herdr-remote-control\""
    ));
    assert!(!script.contains("sudo"));
}

#[test]
fn a_snapshot_names_this_checkout_as_it_is() {
    // Tests run from the checkout they were built from.
    let root = source_root().unwrap();
    let tree = snapshot(&root).unwrap();
    assert_eq!(tree.len(), 40);
    assert!(tree.bytes().all(|byte| byte.is_ascii_hexdigit()));
    // A file not yet added is in it, and the checkout's own index is not
    // touched.
    let listed = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-tree", "-r", "--name-only", &tree])
        .output()
        .unwrap();
    let listed = String::from_utf8(listed.stdout).unwrap();
    assert!(listed.contains("crates/herdr-gpui/src/remote/links/install.rs"));
    assert!(!listed.lines().any(|path| path.starts_with("target/")));
}

/// The probe, copy, revoke, and build scripts, run for real against a
/// temporary home: the probe sees a copy once it is in place, revoking on
/// a host without the program does nothing, and a host without Rust is
/// told so.
#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn the_host_scripts_run_against_a_home() {
    use crate::remote::links::revoke_script;
    use herdr_remote_control::control::DeviceId;
    let dir = tempfile::tempdir().unwrap();
    let build = dir.path().join("build");
    std::fs::write(&build, b"#!/bin/sh\necho \"$@\" > \"$HOME/args\"\n").unwrap();
    let (_, sha256) = host_build::digest(PROGRAM, &build).unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let mut host = Host::new(&herdr_client::ConnectTarget::Local).unwrap();
    // No `cargo` on this PATH, as on a machine without Rust.
    host.env = vec![
        ("HOME".into(), home.to_string_lossy().into_owned()),
        ("PATH".into(), "/usr/bin:/bin".into()),
    ];
    let cancelled = AtomicBool::new(false);
    let limits = ScriptLimits {
        output: 1024,
        idle: Duration::from_secs(10),
    };
    let device = DeviceId("d'1".into());
    // Not through the host: its prelude puts the usual install
    // folders first, where an installed herdr-remote-control would answer.
    // An empty PATH leaves the script none to find.
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let without_program = format!(
        "PATH={}\nexport PATH\n{}",
        shell_quote(&empty.to_string_lossy()),
        revoke_script(&device)
    );
    herdr_client::run_script(
        herdr_client::ScriptHost::Local,
        &without_program,
        &[][..],
        Vec::new(),
        limits,
        &cancelled,
    )
    .unwrap();
    let probe = host.capture(&probe_script("9.9.9"), &cancelled).unwrap();
    assert!(probe.is_empty());
    let file = std::fs::File::open(&build).unwrap();
    host.stream(
        &host_build::copy_script(PROGRAM, "9.9.9", &sha256),
        file,
        Vec::new(),
        limits,
        &cancelled,
    )
    .unwrap();
    let probe = host.capture(&probe_script("9.9.9"), &cancelled).unwrap();
    assert_eq!(String::from_utf8(probe).unwrap().trim(), sha256);
    host.capture(&revoke_script(&device), &cancelled).unwrap();
    assert_eq!(
        std::fs::read_to_string(home.join("args")).unwrap(),
        "revoke --device d'1\n"
    );
    // A source build on a machine without Rust stops with its own status.
    let tarball = {
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("Cargo.toml"), b"[workspace]\n").unwrap();
        let output = std::process::Command::new("tar")
            .args(["-czf", "-", "-C"])
            .arg(&source)
            .arg(".")
            .output()
            .unwrap();
        output.stdout
    };
    // Not through the host: its prelude puts `/opt/homebrew/bin` and
    // `/usr/local/bin` first, where a developer's Rust usually is, so the
    // build would start for real. Only the script decides there is no Rust.
    let without_rust = format!(
        "HOME={}\nexport HOME\nPATH=/usr/bin:/bin\nexport PATH\n{}",
        shell_quote(&home.to_string_lossy()),
        build_script("dev-test")
    );
    let error = herdr_client::run_script(
        herdr_client::ScriptHost::Local,
        &without_rust,
        tarball.as_slice(),
        Vec::new(),
        limits,
        &cancelled,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        herdr_client::Error::ScriptExit { status, .. } if status.code() == Some(NO_RUST_EXIT)
    ));
}
