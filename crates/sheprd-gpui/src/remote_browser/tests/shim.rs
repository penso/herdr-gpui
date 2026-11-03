use super::*;
use crate::remote_browser::install::SHIM;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// A temporary host: the shim, and a fake `herdr` that records its
/// arguments one per line.
struct Host {
    dir: tempfile::TempDir,
}

impl Host {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, text: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, text).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        write("herdr-gpui", SHIM);
        write(
            "herdr",
            "#!/bin/sh\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\"; done >\"$0.args\"\nexit \"${FAKE_HERDR_STATUS:-0}\"\n",
        );
        Self { dir }
    }

    fn herdr(&self) -> PathBuf {
        self.dir.path().join("herdr")
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut command = Command::new("/bin/sh");
        command
            .arg(self.dir.path().join("herdr-gpui"))
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HERDR_BIN_PATH", self.herdr())
            .env("HERDR_WORKSPACE_ID", "w_7")
            .env("HERDR_PANE_ID", "w_7:p_2");
        for (name, value) in env {
            command.env(name, value);
        }
        command.output().unwrap()
    }

    /// What `herdr` was asked, or `None` when it never ran.
    fn args(&self) -> Option<Vec<String>> {
        let text = std::fs::read_to_string(self.dir.path().join("herdr.args")).ok()?;
        Some(text.lines().map(str::to_owned).collect())
    }

    fn forget(&self) {
        let _ = std::fs::remove_file(self.dir.path().join("herdr.args"));
    }
}

/// The tokens a report leaves on the workspace, as Herdr keeps them: each
/// value trimmed and cut to 80 characters, a cleared token gone.
fn reported(args: &[String]) -> Vec<(String, String)> {
    assert_eq!(&args[..3], ["workspace", "report-metadata", "w_7"]);
    let mut tokens = Vec::new();
    for pair in args[3..].windows(2) {
        if pair[0] == "--token" {
            let (name, value) = pair[1].split_once('=').unwrap();
            assert!(name.len() <= 32 && value.chars().count() <= 80, "{name}");
            tokens.push((name.to_owned(), value.trim().to_owned()));
        }
    }
    // Herdr takes at most 16 tokens in one report.
    assert!(tokens.len() <= 16);
    tokens.sort();
    tokens
}

fn request(host: &Host) -> Request {
    let [request] = Request::all(&reported(&host.args().unwrap()))
        .try_into()
        .unwrap();
    request
}

#[test]
fn an_open_reaches_the_window_as_typed() {
    let host = Host::new();
    let output = host.run(&["browser", "open", "localhost:3000/app?x=1"], &[]);
    assert!(output.status.success(), "{output:?}");
    let args = host.args().unwrap();
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--source", "herdr-gpui"])
    );
    assert!(args.windows(2).any(|pair| pair == ["--ttl-ms", "30000"]));
    let request = request(&host);
    assert_eq!(request.pane.as_deref(), Some("w_7:p_2"));
    assert_eq!(
        request.action,
        Action::Open {
            address: "localhost:3000/app?x=1".into(),
            focus: true
        }
    );
}

#[test]
fn a_long_address_travels_in_parts_and_options_are_kept() {
    let host = Host::new();
    let long = format!("https://example.com/{}?q=\"a\\b\"", "x".repeat(700));
    let output = host.run(
        &["--json", "browser", "open", &long, "--no-focus", "--tab"],
        &[],
    );
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["message"].as_str().unwrap().contains(&long));
    assert_eq!(
        request(&host).action,
        Action::Open {
            address: long,
            focus: false
        }
    );

    // Requests made together keep apart: each names its own tokens.
    let first = reported(&host.args().unwrap());
    host.run(
        &["browser", "open", "a.test"],
        &[("HERDR_PANE_ID", "w_7:p_3")],
    );
    let second = reported(&host.args().unwrap());
    assert!(
        first
            .iter()
            .all(|(name, _)| !second.iter().any(|(other, _)| other == name))
    );
    let both: Vec<_> = Request::all(&[first, second].concat())
        .into_iter()
        .map(|request| request.pane)
        .collect();
    assert_eq!(both.len(), 2);
    assert!(both.contains(&Some("w_7:p_3".into())));
}

#[test]
fn reload_names_the_pane_and_no_address() {
    let host = Host::new();
    assert!(host.run(&["browser", "reload"], &[]).status.success());
    let request = request(&host);
    assert_eq!(request.action, Action::Reload);
    assert_eq!(request.pane.as_deref(), Some("w_7:p_2"));
}

/// Arguments, extra environment, and the exit status expected.
type Case<'a> = (&'a [&'a str], &'a [(&'a str, &'a str)], i32);

#[test]
fn what_cannot_travel_is_refused_before_herdr_runs() {
    let host = Host::new();
    let file = host.dir.path().join("page.html");
    std::fs::write(&file, "<p>hi</p>").unwrap();
    let path = file.to_str().unwrap();
    let too_long = format!("https://a.test/{}", "x".repeat(960));
    let cases: [Case; 12] = [
        (&["browser", "open", path], &[], 8),
        (&["browser", "open", "--beside", "a.test"], &[], 8),
        (&["browser", "open", "file:///etc/passwd"], &[], 2),
        (&["browser", "open", "https://a.test/a b"], &[], 2),
        (&["browser", "open", &too_long], &[], 2),
        (&["browser", "open"], &[], 2),
        (&["browser", "open", "a.test", "b.test"], &[], 2),
        (
            &["browser", "open", "a.test"],
            &[("HERDR_WORKSPACE_ID", "")],
            5,
        ),
        (&["browser", "reload"], &[("HERDR_PANE_ID", "")], 1),
        (&["browser", "feedback", "--wait", "60"], &[], 8),
        (&["review"], &[], 8),
        (&["--window", "3", "browser", "open", "a.test"], &[], 8),
    ];
    for (args, env, status) in cases {
        host.forget();
        let output = host.run(args, env);
        assert_eq!(output.status.code(), Some(status), "{args:?}: {output:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
        assert_eq!(host.args(), None, "{args:?}");
    }
    // Errors are JSON when asked for.
    let output = host.run(&["browser", "open", path, "--json"], &[]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["error"]["code"], "unsupported");
}

#[test]
fn a_refusal_from_herdr_is_reported() {
    let host = Host::new();
    let output = host.run(
        &["browser", "open", "a.test"],
        &[("FAKE_HERDR_STATUS", "1")],
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn help_lists_what_works_from_a_host() {
    let host = Host::new();
    for args in [&[][..], &["--help"], &["browser", "--help"]] {
        let output = host.run(args, &[]);
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("sheprd browser open"), "{args:?}");
    }
    assert!(Path::new(&host.herdr()).exists());
}
