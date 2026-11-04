use super::*;
use crate::{paths::Paths, report::By};
use std::{fs, process::ExitCode};

fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

fn output(command: &Command, paths: &Paths) -> (ExitCode, String) {
    let mut out = Vec::new();
    let code = run::run_in(command, paths, &mut out).unwrap();
    (code, String::from_utf8(out).unwrap())
}

const NIGHTLY: &str = r#"
herdr.caps { agents_per_run = 1, tokens_per_day = 2_000_000, wall_time = "3h" }
herdr.at("03:00", { tz = "Europe/Paris" }, function(fire)
  pull("main")
  sh("just ci")
  agent { harness = "claude", worktree = "nightly-" .. fire.date, prompt = "Review main." }
end)
"#;

#[test]
fn parses_every_command() {
    let cases: Vec<(&[&str], Command)> = vec![
        (
            &["check", "a.luau", "--json"],
            Command::Check {
                file: "a.luau".into(),
                json: true,
            },
        ),
        (&["status"], Command::Status { json: false }),
        (&["list", "--json"], Command::Status { json: true }),
        (
            &["approve", "a.luau", "--hash", "abc"],
            Command::Approve {
                file: "a.luau".into(),
                hash: Some("abc".to_owned()),
                json: false,
            },
        ),
        (
            &["run-now", "a.luau", "--trigger", "deploy", "--json"],
            Command::RunNow {
                file: "a.luau".into(),
                trigger: Some("deploy".to_owned()),
                json: true,
            },
        ),
        (
            &["pause", "--all"],
            Command::Pause {
                target: Target::All,
                json: false,
            },
        ),
        (
            &["resume", "a.luau"],
            Command::Resume {
                target: Target::Script("a.luau".into()),
                json: false,
            },
        ),
        (
            &["runs"],
            Command::Runs {
                script: None,
                json: false,
            },
        ),
        (
            &["runs", "show", "7", "--json"],
            Command::RunShow { id: 7, json: true },
        ),
        (
            &["log", "7", "--step", "2"],
            Command::Log {
                target: LogTarget::Run {
                    id: 7,
                    step: Some(2),
                },
                json: false,
                follow: false,
            },
        ),
        (
            &["log", "--daemon", "--lines", "99999", "--follow"],
            Command::Log {
                target: LogTarget::Daemon {
                    lines: MAX_LOG_LINES,
                },
                json: false,
                follow: true,
            },
        ),
        (
            &["stop", "3"],
            Command::Stop {
                run: 3,
                json: false,
            },
        ),
        (
            &["repo", "add", "/r"],
            Command::Repo {
                action: RepoAction::Add("/r".into()),
                json: false,
            },
        ),
        (
            &["repo"],
            Command::Repo {
                action: RepoAction::List,
                json: false,
            },
        ),
        (
            &["usage", "--since", "7d", "--by", "run"],
            Command::Usage {
                script: None,
                since: Since::Week,
                by: By::Run,
                json: false,
            },
        ),
        (&["serve"], Command::Serve),
        (&["start", "--json"], Command::Start { json: true }),
        (&["shutdown"], Command::Shutdown { json: false }),
        (
            &["install-service"],
            Command::InstallService { json: false },
        ),
        (&["watch"], Command::Watch),
        (&["--version"], Command::Version),
        (&["help"], Command::Help),
    ];
    for (input, expected) in cases {
        assert_eq!(parse(args(input)), Ok(expected), "{input:?}");
    }
}

#[test]
fn refuses_bad_usage() {
    assert_eq!(parse(args(&[])), Err(Error::NoCommand));
    assert_eq!(parse(args(&["check"])), Err(Error::Missing("file")));
    assert_eq!(
        parse(args(&["check", "a", "b"])),
        Err(Error::Unexpected("b".to_owned()))
    );
    assert_eq!(
        parse(args(&["status", "--hash", "x"])),
        Err(Error::Unexpected("--hash".to_owned()))
    );
    assert_eq!(
        parse(args(&["stop", "seven"])).unwrap_err().to_string(),
        "`run` does not take `seven`"
    );
    assert_eq!(parse(args(&["runs", "show"])), Err(Error::Missing("run")));
    assert!(parse(args(&["log", "--daemon", "--json", "--follow"])).is_err());
    assert!(parse(args(&["usage", "--since", "1y"])).is_err());
    assert_eq!(
        parse(args(&["chek"])),
        Err(Error::UnknownCommand("chek".to_owned()))
    );
    assert_eq!(
        parse(args(&["serve", "--json"])),
        Err(Error::Unexpected("--json".to_owned()))
    );
}

#[cfg(unix)]
#[test]
fn takes_paths_that_are_not_utf8() {
    use std::os::unix::ffi::OsStringExt;
    let path = OsString::from_vec(vec![b'a', 0xff, b'.', b'l']);
    assert_eq!(
        parse([OsString::from("check"), path.clone()]),
        Ok(Command::Check {
            file: PathBuf::from(path),
            json: false,
        })
    );
}

#[test]
fn check_prints_the_plan_and_its_hash() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::under(dir.path());
    let file = dir.path().join("nightly-validate.luau");
    fs::write(&file, NIGHTLY).unwrap();
    let (code, text) = output(&Command::Check { file, json: false }, &paths);
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(text.starts_with("nightly-validate\n"), "{text}");
    assert!(
        text.contains("    2. sh `just ci` (arbitrary command)\n"),
        "{text}"
    );
    assert!(
        text.contains("OK: 1 trigger. Every trigger can also be started by hand."),
        "{text}"
    );
    assert!(text.contains("sha256 "), "{text}");
}

#[test]
fn errors_are_json_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::under(dir.path());
    let file = dir.path().join("broken.luau");
    fs::write(&file, "herdr.manual(function() end)").unwrap();
    let (code, text) = output(&Command::Check { file, json: true }, &paths);
    assert_eq!(code, ExitCode::FAILURE);
    let value = serde_json::from_str::<serde_json::Value>(&text).unwrap();
    assert_eq!(value["ok"], false);
    assert!(
        value["message"].as_str().unwrap().contains("no caps"),
        "{value}"
    );
    let (code, text) = output(&Command::RunShow { id: 42, json: true }, &paths);
    assert_eq!(code, ExitCode::FAILURE);
    assert!(text.contains("\"message\":\"no run 42\""), "{text}");
}

#[test]
fn status_works_with_no_daemon_and_no_scripts() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::under(dir.path());
    let (code, text) = output(&Command::Status { json: true }, &paths);
    assert_eq!(code, ExitCode::SUCCESS);
    let status = serde_json::from_str::<sheprd_automation::report::Status>(&text).unwrap();
    assert!(!status.daemon.running);
    assert!(status.scripts.is_empty());
    assert_eq!(status.protocol, sheprd_automation::report::PROTOCOL);
}

#[test]
fn steps_lists_the_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let (_, text) = output(&Command::Steps { json: true }, &Paths::under(dir.path()));
    let value = serde_json::from_str::<serde_json::Value>(&text).unwrap();
    let names = value["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let expected = sheprd_automation::Kind::ALL
        .iter()
        .map(|kind| kind.name().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
}

#[cfg(unix)]
#[test]
fn a_script_is_approved_by_hash_then_can_be_run_by_hand() {
    use crate::approve::{self, Mode};
    let fixture = crate::testing::Fixture::new();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "deploy",
        "herdr.manual(function() notify(\"go\") end)\n",
    );
    let paths = &fixture.paths;
    let repo_add = Command::Repo {
        action: RepoAction::Add(repo.clone()),
        json: false,
    };
    assert_eq!(output(&repo_add, paths).0, ExitCode::SUCCESS);
    let store = fixture.store();
    let source = sheprd_automation::Source::read(&script).unwrap();
    let approve = |hash: &str, pane: Option<&str>| {
        approve::approve(
            paths,
            &store,
            &fixture.herdr,
            &script,
            Mode::Hash(hash),
            pane,
        )
    };
    assert!(matches!(
        approve(&"0".repeat(64), None),
        Err(crate::Error::Approve(approve::Error::Changed { .. }))
    ));
    assert!(matches!(
        approve(&source.hash, Some("w1p1")),
        Err(crate::Error::Approve(approve::Error::HashInPane))
    ));
    assert_eq!(approve(&source.hash, None).unwrap().hash, source.hash);
    let (code, text) = output(&Command::Status { json: true }, paths);
    assert_eq!(code, ExitCode::SUCCESS);
    let status = serde_json::from_str::<sheprd_automation::report::Status>(&text).unwrap();
    assert_eq!(
        status.scripts.first().map(|script| script.state),
        Some(sheprd_automation::report::ScriptState::Active)
    );
    let run_now = Command::RunNow {
        file: script.clone(),
        trigger: None,
        json: true,
    };
    let (code, text) = output(&run_now, paths);
    assert_eq!(code, ExitCode::SUCCESS, "{text}");
    assert!(text.contains("the daemon is not running"), "{text}");
    // Changing the script takes the approval away.
    fs::write(
        &script,
        format!("{}-- changed\n", fs::read_to_string(&script).unwrap()),
    )
    .unwrap();
    store
        .put_script(&crate::scan::plan_row(
            &sheprd_automation::Source::read(&script).unwrap(),
            script
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            None,
            0,
        ))
        .unwrap();
    let (code, text) = output(&run_now, paths);
    assert_eq!(code, ExitCode::FAILURE);
    assert!(text.contains("not approved"), "{text}");
}

#[test]
fn commands_that_leave_scripts_to_run_start_the_daemon() {
    let dir = tempfile::tempdir().unwrap();
    let with_scripts = dir.path().join("with");
    fs::create_dir_all(with_scripts.join(".herdr/automations")).unwrap();
    fs::write(with_scripts.join(".herdr/automations/nightly.luau"), "").unwrap();
    let without = dir.path().join("without");
    fs::create_dir_all(&without).unwrap();
    let repo_add = |repo: &std::path::Path| Command::Repo {
        action: RepoAction::Add(repo.to_path_buf()),
        json: false,
    };
    assert!(run::leaves_scripts_to_run(&repo_add(&with_scripts)));
    assert!(!run::leaves_scripts_to_run(&repo_add(&without)));
    for command in [
        Command::Approve {
            file: "a.luau".into(),
            hash: None,
            json: true,
        },
        Command::RunNow {
            file: "a.luau".into(),
            trigger: None,
            json: false,
        },
    ] {
        assert!(run::leaves_scripts_to_run(&command), "{command:?}");
    }
    for command in [
        Command::Status { json: false },
        Command::Steps { json: false },
    ] {
        assert!(!run::leaves_scripts_to_run(&command), "{command:?}");
    }
}
