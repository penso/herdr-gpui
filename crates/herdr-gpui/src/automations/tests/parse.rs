//! Reading what `herdr-automate --json` prints, and the commands sent to it.
use super::*;
use crate::automations::{
    fetch,
    install::{self, Platform},
};
use herdr_automation::report::{Done, LogText, RunDetail};

fn exit(code: i32) -> herdr_client::Error {
    #[cfg(unix)]
    let status = {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code << 8)
    };
    #[cfg(windows)]
    let status = {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code as u32)
    };
    herdr_client::Error::ScriptExit {
        status,
        stderr: String::new(),
    }
}

#[test]
fn a_status_round_trips_through_the_report_types() {
    let status = status();
    let json = serde_json::to_vec(&status).unwrap();
    let read: Status = fetch::parse(&json).unwrap();
    assert_eq!(read, status);
    assert_eq!(fetch::checked(read).unwrap(), status);
}

#[test]
fn a_newer_protocol_is_refused_rather_than_misread() {
    let mut newer = status();
    newer.protocol = herdr_automation::report::PROTOCOL + 1;
    assert!(matches!(
        fetch::checked(newer),
        Err(Error::Protocol { found, expected })
            if found == expected + 1 && expected == herdr_automation::report::PROTOCOL
    ));
}

#[test]
fn malformed_output_is_a_json_error() {
    for output in [&b""[..], b"not json", b"{\"protocol\": 1}", b"[1,2]"] {
        assert!(
            matches!(fetch::parse::<Status>(output), Err(Error::Json(_))),
            "{output:?}"
        );
    }
}

#[test]
fn the_other_reports_parse() {
    let detail = RunDetail {
        run: run(7, "nightly", RunState::Running, NOW),
        steps: Vec::new(),
    };
    let json = serde_json::to_vec(&detail).unwrap();
    assert_eq!(fetch::parse::<RunDetail>(&json).unwrap(), detail);
    let log: LogText = fetch::parse(br#"{"text":"a\nb","truncated":true}"#).unwrap();
    assert!(log.truncated);
    assert_eq!(log.text.lines().count(), 2);
}

#[test]
fn failures_are_classified() {
    // A shell that cannot find the program.
    assert!(matches!(
        fetch::classify(exit(127), b"", false),
        Error::Missing
    ));
    // This machine has no such program.
    let spawn = herdr_client::Error::ScriptSpawn(std::io::ErrorKind::NotFound.into());
    assert!(matches!(fetch::classify(spawn, b"", true), Error::Missing));
    // The program's own reason, stripped of control characters.
    let done = serde_json::to_vec(&Done {
        ok: false,
        message: "no such\u{1b}[31m script".to_owned(),
        run: None,
    })
    .unwrap();
    let Error::Refused(message) = fetch::classify(exit(1), &done, false) else {
        panic!("not a refusal");
    };
    assert_eq!(message, "no such[31m script");
    // An exit without a report keeps the transport's error and its source.
    let error = fetch::classify(exit(1), b"garbage", false);
    assert!(matches!(
        error,
        Error::Run(herdr_client::Error::ScriptExit { .. })
    ));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn versions_are_checked_before_they_name_a_directory() {
    assert_eq!(
        fetch::parse_version(b"herdr-automate 0.3.1\n").unwrap(),
        "0.3.1"
    );
    for bad in [
        &b"herdr-automate"[..],
        b"herdr-automate ../../x",
        b"other 1.0",
        b"",
    ] {
        assert!(fetch::parse_version(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn command_lines_quote_every_word() {
    assert_eq!(
        fetch::line(&["approve", "/a b/it's.luau", "--json"]),
        "'herdr-automate' 'approve' '/a b/it'\\''s.luau' '--json'\n"
    );
    let approve = Action::Approve {
        path: "/x.luau".into(),
        hash: "abc".into(),
    };
    assert_eq!(approve.args(), ["approve", "/x.luau", "--hash", "abc"]);
    assert_eq!(Action::PauseAll.args(), ["pause", "--all"]);
    assert_eq!(Action::Stop { run: 12 }.args(), ["stop", "12"]);
}

#[test]
fn platforms_come_from_uname() {
    assert_eq!(
        Platform::parse("Linux x86_64\n").unwrap().triple(),
        "x86_64-unknown-linux-musl"
    );
    assert_eq!(
        Platform::parse("Linux aarch64").unwrap().triple(),
        "aarch64-unknown-linux-musl"
    );
    assert_eq!(
        Platform::parse("Darwin arm64").unwrap().triple(),
        "aarch64-apple-darwin"
    );
    for bad in ["", "Linux", "FreeBSD amd64", "Linux riscv64"] {
        assert!(
            matches!(
                Platform::parse(bad),
                Err(crate::host_build::Error::Platform(_))
            ),
            "{bad}"
        );
    }
}

#[test]
fn a_build_is_this_apps_own_or_one_shipped_for_the_platform() {
    let dir = tempfile::tempdir().unwrap();
    let local = Platform::local();
    let own = dir.path().join("herdr-automate");
    std::fs::write(&own, b"own").unwrap();
    assert_eq!(
        install::build_for(&local, dir.path(), Some(own.clone())),
        Some(own.clone())
    );
    let other = Platform {
        os: if local.os == "linux" {
            "macos"
        } else {
            "linux"
        },
        arch: "x86_64",
    };
    // No build for another platform is never this computer's.
    assert_eq!(install::build_for(&other, dir.path(), Some(own)), None);
    let shipped = dir
        .path()
        .join("automate")
        .join(other.triple())
        .join("herdr-automate");
    std::fs::create_dir_all(shipped.parent().unwrap()).unwrap();
    std::fs::write(&shipped, b"other").unwrap();
    assert_eq!(install::build_for(&other, dir.path(), None), Some(shipped));
}

#[test]
fn the_install_script_checks_the_copy_and_stays_in_home() {
    let dir = tempfile::tempdir().unwrap();
    let build = dir.path().join("herdr-automate");
    std::fs::write(&build, b"hello").unwrap();
    let (size, sha256) = install::digest(&build).unwrap();
    assert_eq!(size, 5);
    assert_eq!(
        sha256,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    let plan = install::Plan {
        platform: Platform::local(),
        source: build,
        version: "0.3.1".into(),
        size,
        sha256,
    };
    let script = install::script(&plan);
    assert!(script.contains("version='0.3.1'"));
    assert!(
        script.contains("program=herdr-automate") || script.contains("program='herdr-automate'")
    );
    assert!(script.contains("$HOME/.local/share/$program/bin/$version"));
    assert!(script.contains("ln -sfn \"$dir/$program\" \"$HOME/.local/bin/$program\""));
    assert!(script.contains("exit 3"));
    assert!(!script.contains("sudo"));
}

/// The script, run for real against a temporary home: the copy lands and
/// is linked, and a wrong checksum leaves nothing behind.
#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn the_install_script_puts_a_checked_copy_in_place() {
    use std::{sync::atomic::AtomicBool, time::Duration};
    let dir = tempfile::tempdir().unwrap();
    let build = dir.path().join("build");
    std::fs::write(&build, b"#!/bin/sh\necho herdr-automate 9.9.9\n").unwrap();
    let (size, sha256) = install::digest(&build).unwrap();
    let mut plan = install::Plan {
        platform: Platform::local(),
        source: build.clone(),
        version: "9.9.9".into(),
        size,
        sha256,
    };
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let mut host = Host::new(&herdr_client::ConnectTarget::Local).unwrap();
    host.env = vec![("HOME".into(), home.to_string_lossy().into_owned())];
    let cancelled = AtomicBool::new(false);
    let limits = herdr_client::ScriptLimits {
        output: 1024,
        idle: Duration::from_secs(10),
    };
    let run = |plan: &install::Plan| {
        let file = std::fs::File::open(&plan.source).unwrap();
        host.stream(&install::script(plan), file, Vec::new(), limits, &cancelled)
    };
    run(&plan).unwrap();
    let installed = home.join(".local/share/herdr-automate/bin/9.9.9/herdr-automate");
    assert_eq!(
        std::fs::read(&installed).unwrap(),
        std::fs::read(&build).unwrap()
    );
    assert_eq!(
        std::fs::read_link(home.join(".local/bin/herdr-automate")).unwrap(),
        installed
    );
    plan.version = "1.0.0".into();
    plan.sha256 = "0".repeat(64);
    let refused = run(&plan).unwrap_err();
    assert!(matches!(
        refused,
        herdr_client::Error::ScriptExit { status, .. } if status.code() == Some(3)
    ));
    let left: Vec<_> = std::fs::read_dir(home.join(".local/share/herdr-automate/bin/1.0.0"))
        .unwrap()
        .collect();
    assert!(left.is_empty(), "{left:?}");
}
