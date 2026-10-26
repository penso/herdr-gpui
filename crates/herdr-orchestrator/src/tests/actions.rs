use super::*;
use crate::actions::{Action, DispatchRequest, Site, perform};
use herdr_client::ConnectTarget;
use herdr_remote::teleport::{AgentKind, Host};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{OnceLock, atomic::AtomicBool},
};

mod agent_name;
mod parent;
mod remove;
mod startup;

/// `tests/support/fake_herdr.rs`, compiled once per test process: a program
/// rather than a script, so the dispatch tests run on every client platform.
fn fake_herdr() -> &'static Path {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT.get_or_init(|| {
        let dir =
            std::env::temp_dir().join(format!("herdr-gpui-fake-herdr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join(format!("herdr{}", std::env::consts::EXE_SUFFIX));
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_herdr.rs");
        // An earlier process with this ID left its program here. Rewriting
        // it in place keeps that inode's cached code signature on macOS,
        // which then kills the new program (signal 9); a new file has none.
        match std::fs::remove_file(&program) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("removing {}: {error}", program.display()),
        }
        // `rustc` writes the program, so this process never holds it open for
        // writing while a sibling test forks (Linux would refuse to run it).
        let status = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--edition", "2021", "-o"])
            .arg(&program)
            .arg(&source)
            .status()
            .unwrap();
        assert!(status.success(), "compiling {}", source.display());
        program
    })
}

/// A home whose `herdr` (see `tests/support/fake_herdr.rs`) logs each call
/// and answers as Herdr would. A call whose arguments contain `fail` exits 1.
/// Its snapshot is `$HOME/snapshot` when written, else one where `w1` is the
/// repository's parent workspace and `w9` the linked worktree a dispatch
/// creates.
fn fake_home(dir: &Path, fail: &str) -> Host {
    let home = dir.join("home");
    let bin = home.join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        home.join("snapshot-default"),
        snapshot(&[("w1", false), ("w9", true)]),
    )
    .unwrap();
    std::fs::write(home.join("fail"), fail).unwrap();
    // A link, not a copy, for the same reason `rustc` writes the program.
    std::fs::hard_link(
        fake_herdr(),
        bin.join(format!("herdr{}", std::env::consts::EXE_SUFFIX)),
    )
    .unwrap();
    let mut host = Host::new(&ConnectTarget::Local).unwrap();
    host.env = vec![("HOME".into(), home.to_string_lossy().into_owned())];
    host
}

/// `herdr api snapshot` output with each `(workspace, linked)` a checkout of
/// one repository.
fn snapshot(workspaces: &[(&str, bool)]) -> String {
    let workspaces: Vec<_> = workspaces
        .iter()
        .map(|(id, linked)| {
            serde_json::json!({
                "workspace_id": id,
                "worktree": {
                    "repo_key": "/r/.git",
                    "repo_name": "r",
                    "repo_root": "/r",
                    "checkout_path": format!("/r/{id}"),
                    "is_linked_worktree": linked,
                },
            })
        })
        .collect();
    serde_json::json!({
        "id": "cli:api:snapshot",
        "result": {"snapshot": {"workspaces": workspaces, "tabs": [], "panes": []}},
    })
    .to_string()
}

fn repository(dir: &Path) -> String {
    let root = dir.join("app");
    std::fs::create_dir_all(&root).unwrap();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
    git(&["init", "-q"]);
    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "base",
    ]);
    root.to_string_lossy().into_owned()
}

fn site(dir: &Path, fail: &str) -> Site {
    Site {
        host: fake_home(dir, fail),
        target: ConnectTarget::Local,
        main_root: repository(dir),
        database: dir.join("db/id/state.sqlite3"),
    }
}

fn request() -> DispatchRequest {
    DispatchRequest {
        item_key: "github:github.com:penso/herdr-gpui:377".into(),
        kind: AgentKind::Claude,
        model: None,
        prompt: "Fix the icons; don't break 'quotes'.".into(),
        branch: "377-icons".into(),
        workspace_id: "w1".into(),
        base: None,
        elsewhere: None,
    }
}

fn calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("home/calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn dispatch_creates_the_worktree_starts_the_agent_and_records_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let done = perform(
        &site,
        &Action::Dispatch(Box::new(request())),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(done, "Agent started");
    let calls = calls(dir.path());
    assert_eq!(calls[0], "api snapshot");
    assert!(calls[1].starts_with("worktree create --workspace w1 --branch 377-icons --base "));
    assert!(calls[1].ends_with(" --no-focus"));
    let name = calls[2].split(' ').nth(2).unwrap();
    assert!(
        name.starts_with("herdr-gpui-") && name.len() == 32,
        "{name}"
    );
    assert_eq!(
        calls[2],
        format!("agent start {name} --kind claude --pane p9 --timeout 90000")
    );
    assert_eq!(
        calls[3],
        "agent prompt p9 Fix the icons; don't break 'quotes'."
    );

    let store = Store::open(&site.database).unwrap();
    let run = store.runs().unwrap().remove(0);
    assert_eq!(
        (run.owner, run.state, run.agent.as_str()),
        (Owner::HerdrGpui, RunState::Running, "claude")
    );
    let workspace = run.workspace.unwrap();
    assert_eq!(
        (workspace.id.as_str(), workspace.branch.as_str()),
        ("w9", "377-icons")
    );
    let session = store.sessions().unwrap().remove(0);
    assert_eq!(
        (session.pane_id.as_str(), session.agent_name.as_str()),
        ("p9", name)
    );
    assert_eq!(session.host, None);
}

#[test]
fn a_failed_start_removes_the_worktree_and_records_why() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "agent start");
    let error = perform(
        &site,
        &Action::Dispatch(Box::new(request())),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        Error::Script {
            operation: "Starting the agent",
            ..
        }
    ));
    assert_eq!(
        calls(dir.path()).last().unwrap(),
        "worktree remove --workspace w9 --force"
    );
    let run = Store::open(&site.database)
        .unwrap()
        .runs()
        .unwrap()
        .remove(0);
    assert_eq!(run.state, RunState::Failed);
    assert!(run.message.unwrap().contains("Starting the agent"));
}

#[test]
fn stop_and_remove_act_on_own_runs_only() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let cancelled = AtomicBool::new(false);
    perform(&site, &Action::Dispatch(Box::new(request())), &cancelled).unwrap();
    let id = Store::open(&site.database).unwrap().runs().unwrap()[0]
        .id
        .clone();

    perform(&site, &Action::Stop { run: id.clone() }, &cancelled).unwrap();
    let calls_now = calls(dir.path());
    assert_eq!(
        &calls_now[calls_now.len() - 2..],
        ["agent send-keys p9 esc", "agent send-keys p9 ctrl+c"]
    );
    let run = Store::open(&site.database)
        .unwrap()
        .runs()
        .unwrap()
        .remove(0);
    assert_eq!(run.state, RunState::Cancelled);

    perform(&site, &Action::Remove { run: id.clone() }, &cancelled).unwrap();
    assert_eq!(
        calls(dir.path()).last().unwrap(),
        "worktree remove --workspace w9 --force"
    );
    assert!(
        Store::open(&site.database)
            .unwrap()
            .runs()
            .unwrap()
            .is_empty()
    );

    // agent-launcher's run is shown, never acted on.
    rusqlite::Connection::open(&site.database)
        .unwrap()
        .execute_batch(
            "INSERT INTO runs (id, issue_key, agent, state_json, started_at, updated_at) VALUES \
             ('theirs', 'k', 'opencode', '\"running\"', '2026-10-08T00:00:00+00:00', '2026-10-08T00:00:00+00:00')",
        )
        .unwrap();
    let before = calls(dir.path()).len();
    for action in [
        Action::Stop {
            run: "theirs".into(),
        },
        Action::Remove {
            run: "theirs".into(),
        },
        Action::Send {
            run: "theirs".into(),
            text: "hi".into(),
        },
    ] {
        assert!(matches!(
            perform(&site, &action, &cancelled),
            Err(Error::NotOwner(_))
        ));
    }
    assert_eq!(
        calls(dir.path()).len(),
        before,
        "nothing ran for a foreign run"
    );
}

#[test]
fn a_chosen_model_reaches_the_agent_and_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let mut with_model = request();
    with_model.model = Some("claude-opus-5-5".into());
    perform(
        &site,
        &Action::Dispatch(Box::new(with_model)),
        &AtomicBool::new(false),
    )
    .unwrap();
    let start = calls(dir.path())[2].clone();
    assert!(
        start.ends_with(" --timeout 90000 -- --model claude-opus-5-5"),
        "{start}"
    );
    let run = Store::open(&site.database)
        .unwrap()
        .runs()
        .unwrap()
        .remove(0);
    assert_eq!(run.model.as_deref(), Some("claude-opus-5-5"));
}

#[test]
fn only_plain_model_names_are_passed_on() {
    use crate::actions::valid_model;
    for good in [
        "gpt-5.5",
        "anthropic/claude-sonnet-5-5",
        "qwen3:32b",
        "model@latest",
    ] {
        assert!(valid_model(good), "{good}");
    }
    for bad in [
        "",
        "--dangerously-skip-permissions",
        "a b",
        "x;rm",
        &"m".repeat(129),
    ] {
        assert!(!valid_model(bad), "{bad}");
    }
}

#[test]
fn a_run_dispatched_elsewhere_is_reached_on_its_own_host() {
    use crate::actions::target_of;
    let mut on = session("r");
    on.host = Some("devbox".into());
    on.session = Some("work".into());
    assert_eq!(
        target_of(&on),
        ConnectTarget::Ssh {
            target: "devbox".into(),
            session: "work".into()
        }
    );
    on.session = None;
    assert_eq!(
        target_of(&on),
        ConnectTarget::Ssh {
            target: "devbox".into(),
            session: "default".into()
        }
    );
    on.host = None;
    assert_eq!(target_of(&on), ConnectTarget::Local);
    on.session = Some("dev".into());
    assert_eq!(
        target_of(&on),
        ConnectTarget::Session {
            name: "dev".into(),
            development: false
        }
    );
}
