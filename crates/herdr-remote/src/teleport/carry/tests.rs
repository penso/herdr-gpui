use super::*;
use super::{review_repository as review, run_repository as run};

mod session_only;
use crate::teleport::Host;
use herdr_client::ConnectTarget;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

const ORIGIN_URL: &str = "https://github.com/acme/app.git";

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn commit(dir: &Path, file: &str) -> String {
    fs::write(dir.join(file), file).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", file]);
    git(dir, &["rev-parse", "HEAD"])
}

/// A stand-in `herdr` for both hosts. The source answers from files in its
/// home; the destination creates and removes real worktrees of `$REPO`.
/// Every call is logged.
const HERDR: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/log"
case "$1 $2" in
"api snapshot") cat "$HOME/snapshot.json" ;;
"pane process-info") cat "$HOME/process.json" ;;
"pane close") ;;
"pane run") [ ! -e "$HOME/refuse-run" ] || { echo "run refused" >&2; exit 1; } ;;
"tab create") printf '%s\n' '{"result":{"tab":{"tab_id":"w5:t2"},"root_pane":{"pane_id":"w5:p3"}}}' ;;
"tab close") ;;
"worktree create")
    path="$HOME/worktree"
    shift 2
    branch= base=
    while [ $# -gt 0 ]; do
        case "$1" in --branch) branch=$2; shift ;; --base) base=$2; shift ;; esac
        shift
    done
    if [ -n "$branch" ]; then
        git -C "$REPO" worktree add -q "$path" "$branch" >&2
    else
        git -C "$REPO" worktree add -q -b carried "$path" "$base" >&2
    fi
    printf '{"result":{"workspace":{"workspace_id":"w9"},"tab":{"tab_id":"w9:t1"},"root_pane":{"pane_id":"w9:p1"},"worktree":{"path":"%s"}}}\n' "$path" ;;
"worktree remove") git -C "$REPO" worktree remove --force "$HOME/worktree" >&2 ;;
*) echo "unexpected: $*" >&2; exit 2 ;;
esac
"#;

struct Fixture {
    _temp: tempfile::TempDir,
    main: PathBuf,
    linked: PathBuf,
    destination: PathBuf,
    source_home: PathBuf,
    destination_home: PathBuf,
    origin: Origin,
    hosts: HostRepositories,
}

fn host(home: &Path, repo: &Path) -> Host {
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    let herdr = bin.join("herdr");
    fs::write(&herdr, HERDR).unwrap();
    fs::set_permissions(&herdr, fs::Permissions::from_mode(0o755)).unwrap();
    let mut host = Host::new(&ConnectTarget::Local).unwrap();
    host.env = [
        ("HOME", home.to_string_lossy().into_owned()),
        ("REPO", repo.to_string_lossy().into_owned()),
        ("PATH", "/usr/bin:/bin".into()),
        ("GIT_CONFIG_GLOBAL", "/dev/null".into()),
        ("GIT_CONFIG_NOSYSTEM", "1".into()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value))
    .collect();
    host
}

/// A source repository whose `feature` checkout is a commit ahead with
/// staged, unstaged, and untracked changes, a destination clone of it that
/// predates that commit, and a pane running `make watch` in `checkout/sub`.
fn fixture(checkout: impl FnOnce(&Fixture) -> PathBuf) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let main = temp.path().join("source repo");
    fs::create_dir(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    git(&main, &["remote", "add", "origin", ORIGIN_URL]);
    commit(&main, "base.txt");
    let destination = temp.path().join("destination repo");
    git(
        temp.path(),
        &[
            "clone",
            "-q",
            main.to_str().unwrap(),
            destination.to_str().unwrap(),
        ],
    );
    git(&destination, &["remote", "set-url", "origin", ORIGIN_URL]);
    let linked = temp.path().join("feature checkout");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            linked.to_str().unwrap(),
        ],
    );
    commit(&linked, "feature.txt");
    fs::write(linked.join("base.txt"), "changed").unwrap();
    fs::write(linked.join("staged.txt"), "staged").unwrap();
    git(&linked, &["add", "staged.txt"]);
    fs::write(linked.join("notes.txt"), "untracked").unwrap();
    fs::create_dir(linked.join("sub")).unwrap();
    fs::create_dir(main.join("sub")).unwrap();
    let source_home = temp.path().join("source home");
    let destination_home = temp.path().join("destination home");
    let mut fixture = Fixture {
        origin: Origin {
            place: Place {
                endpoint_id: "local".into(),
                label: "Here".into(),
                host: host(&source_home, &main),
            },
            workspace_id: "w1".into(),
            pane_id: "w1:p2".into(),
        },
        hosts: HostRepositories {
            place: Place {
                endpoint_id: "ssh:there".into(),
                label: "There".into(),
                host: host(&destination_home, &destination),
            },
            repositories: Some(vec![Repository {
                key: destination.join(".git").to_string_lossy().into_owned(),
                label: "app".into(),
                workspace_id: "w5".into(),
            }]),
            retired: Vec::new(),
        },
        main,
        linked,
        destination,
        source_home,
        destination_home,
        _temp: temp,
    };
    let checkout = checkout(&fixture);
    let checkout = checkout.to_str().unwrap();
    let snapshot = serde_json::json!({"result": {"snapshot": {
        "workspaces": [{"workspace_id": "w1", "worktree": {
            "repo_key": fixture.main.join(".git"), "repo_name": "app",
            "repo_root": fixture.main, "checkout_path": checkout,
            "is_linked_worktree": checkout != fixture.main.to_str().unwrap(),
        }}],
        "tabs": [{"tab_id": "w1:t1", "workspace_id": "w1", "number": 1}],
        "panes": [{"pane_id": "w1:p2", "tab_id": "w1:t1", "cwd": checkout}],
    }}});
    fs::write(
        fixture.source_home.join("snapshot.json"),
        snapshot.to_string(),
    )
    .unwrap();
    let process = serde_json::json!({"result": {"process_info": {
        "foreground_process_group_id": 42, "shell_pid": 7,
        "foreground_processes": [{"pid": 42, "argv": ["make", "watch"], "cwd": format!("{checkout}/sub")}],
    }}});
    fs::write(
        fixture.source_home.join("process.json"),
        process.to_string(),
    )
    .unwrap();
    // The destination's main checkout is open as the space `w5`.
    let space = serde_json::json!({"result": {"snapshot": {
        "workspaces": [{"workspace_id": "w5", "worktree": {
            "repo_key": fixture.hosts.repositories.as_ref().unwrap()[0].key,
            "repo_name": "app", "repo_root": fixture.destination,
            "checkout_path": fixture.destination, "is_linked_worktree": false,
        }}],
        "tabs": [], "panes": [],
    }}});
    fs::create_dir_all(&fixture.destination_home).unwrap();
    fs::write(
        fixture.destination_home.join("snapshot.json"),
        space.to_string(),
    )
    .unwrap();
    fixture.origin.place.host = host(&fixture.source_home, &fixture.main);
    fixture
}

fn log(home: &Path) -> String {
    fs::read_to_string(home.join("log")).unwrap_or_default()
}

fn work_references(repo: &Path) -> String {
    git(repo, &["for-each-ref", "refs/herdr-teleport"])
}

#[test]
fn a_pane_carries_its_branch_and_changes_and_closes_at_the_source() {
    let fixture = fixture(|fixture| fixture.linked.clone());
    let cancelled = AtomicBool::new(false);
    let before = git(&fixture.linked, &["status", "--porcelain"]);
    let head = git(&fixture.linked, &["rev-parse", "HEAD"]);

    let review = review(
        &fixture.origin,
        &fixture.hosts,
        &Choice::NewWorktree,
        &cancelled,
    )
    .unwrap();
    assert_eq!(
        review.landing,
        Landing::Worktree {
            branch: Some("feature".into())
        }
    );
    assert_eq!(
        review.action,
        Action::Run(vec!["make".into(), "watch".into()])
    );
    assert_eq!((review.state.changed, review.state.untracked), (3, 1));

    let mut steps = Vec::new();
    let carried = run(
        &fixture.origin,
        &review,
        |step| steps.push(step),
        &cancelled,
    )
    .unwrap();
    assert_eq!(
        (
            carried.endpoint_id.as_str(),
            carried.workspace_id.as_str(),
            carried.pane_id.as_str()
        ),
        ("ssh:there", "w9", "w9:p1")
    );
    assert!(carried.warnings.is_empty(), "{:?}", carried.warnings);
    assert_eq!(steps.first(), Some(&Step::Capture));
    assert_eq!(steps.last(), Some(&Step::Retire));

    let landed = fixture.destination_home.join("worktree");
    assert_eq!(git(&landed, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&landed, &["branch", "--show-current"]), "feature");
    assert_eq!(git(&landed, &["status", "--porcelain"]), before);
    assert_eq!(
        fs::read_to_string(landed.join("base.txt")).unwrap(),
        "changed"
    );
    let launched = log(&fixture.destination_home);
    let line = launched
        .lines()
        .find(|line| line.starts_with("pane run w9:p1 "))
        .unwrap();
    assert!(
        line.contains(&format!("{}/sub", landed.display())),
        "{line}"
    );
    assert!(line.ends_with("make watch"), "{line}");

    // The source keeps its checkout exactly; only the pane closes.
    assert_eq!(git(&fixture.linked, &["status", "--porcelain"]), before);
    assert_eq!(git(&fixture.linked, &["rev-parse", "HEAD"]), head);
    assert!(log(&fixture.source_home).contains("pane close w1:p2\n"));
    assert_eq!(work_references(&fixture.main), "");
    assert_eq!(work_references(&fixture.destination), "");
}

/// A pane in the main checkout copies its work too. Its branch is checked
/// out on the destination already, so the new worktree gets its own.
#[test]
fn a_branch_the_destination_has_checked_out_starts_a_new_one() {
    let fixture = fixture(|fixture| fixture.main.clone());
    fs::write(fixture.main.join("base.txt"), "edited on main").unwrap();
    let cancelled = AtomicBool::new(false);
    let review = review(
        &fixture.origin,
        &fixture.hosts,
        &Choice::NewWorktree,
        &cancelled,
    )
    .unwrap();
    assert_eq!(review.landing, Landing::Worktree { branch: None });
    run(&fixture.origin, &review, |_| {}, &cancelled).unwrap();
    let landed = fixture.destination_home.join("worktree");
    assert_eq!(
        git(&landed, &["rev-parse", "HEAD"]),
        git(&fixture.main, &["rev-parse", "HEAD"])
    );
    assert_eq!(git(&landed, &["branch", "--show-current"]), "carried");
    assert_eq!(
        fs::read_to_string(landed.join("base.txt")).unwrap(),
        "edited on main"
    );
    assert_eq!(
        git(&fixture.destination, &["branch", "--show-current"]),
        "main"
    );
}

/// Until the pane runs there, the new worktree holds only copied work, so a
/// failure removes it and leaves the source pane open.
#[test]
fn a_failed_launch_removes_the_new_worktree() {
    let fixture = fixture(|fixture| fixture.linked.clone());
    fs::write(fixture.destination_home.join("refuse-run"), "").unwrap();
    let cancelled = AtomicBool::new(false);
    let review = review(
        &fixture.origin,
        &fixture.hosts,
        &Choice::NewWorktree,
        &cancelled,
    )
    .unwrap();
    let error = run(&fixture.origin, &review, |_| {}, &cancelled).unwrap_err();
    assert!(
        matches!(
            error,
            Error::Script {
                step: Step::Launch,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(!fixture.destination_home.join("worktree").exists());
    assert!(log(&fixture.destination_home).contains("worktree remove --workspace w9 --force"));
    assert!(!log(&fixture.source_home).contains("pane close"));
    assert_eq!(work_references(&fixture.destination), "");
}

/// Only a repository sharing a remote is the same project.
#[test]
fn a_repository_matching_only_by_name_is_refused() {
    let fixture = fixture(|fixture| fixture.linked.clone());
    git(
        &fixture.destination,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/other/app.git",
        ],
    );
    let error = review(
        &fixture.origin,
        &fixture.hosts,
        &Choice::NewWorktree,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::RepositoryNotOpen { host } if host == "There"),
        "{error:?}"
    );
}

fn space() -> Choice {
    Choice::Space {
        workspace_id: "w5".into(),
    }
}

/// A space behind the pane on the same branch fast-forwards, takes the
/// changes, and runs the pane in a new tab; the source keeps its work.
#[test]
fn a_space_behind_on_the_same_branch_fast_forwards_and_takes_the_changes() {
    let fixture = fixture(|fixture| fixture.main.clone());
    let head = commit(&fixture.main, "ahead.txt");
    fs::write(fixture.main.join("base.txt"), "edited on main").unwrap();
    let before = git(&fixture.main, &["status", "--porcelain"]);
    let cancelled = AtomicBool::new(false);
    let review = review(&fixture.origin, &fixture.hosts, &space(), &cancelled).unwrap();
    let Landing::Space {
        workspace_id,
        fast_forward,
        ..
    } = &review.landing
    else {
        panic!("{:?}", review.landing);
    };
    assert_eq!((workspace_id.as_str(), *fast_forward), ("w5", true));
    let carried = run(&fixture.origin, &review, |_| {}, &cancelled).unwrap();
    assert_eq!(
        (carried.workspace_id.as_str(), carried.pane_id.as_str()),
        ("w5", "w5:p3")
    );
    assert_eq!(git(&fixture.destination, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git(&fixture.destination, &["status", "--porcelain"]),
        before
    );
    let launched = log(&fixture.destination_home);
    assert!(
        launched.contains("tab create --workspace w5 --cwd "),
        "{launched}"
    );
    assert!(launched.contains("pane run w5:p3 "), "{launched}");
    assert_eq!(git(&fixture.main, &["status", "--porcelain"]), before);
    assert!(log(&fixture.source_home).contains("pane close w1:p2\n"));
    assert_eq!(work_references(&fixture.destination), "");
}

/// A space is never overwritten: uncommitted work there, another branch, or
/// a branch that has moved on all refuse the move before anything copies.
#[test]
fn a_space_with_its_own_work_is_refused() {
    let cancelled = AtomicBool::new(false);
    let f = fixture(|fixture| fixture.main.clone());
    fs::write(f.destination.join("base.txt"), "theirs").unwrap();
    let error = review(&f.origin, &f.hosts, &space(), &cancelled).unwrap_err();
    assert!(matches!(error, Error::SpaceChanged { .. }), "{error:?}");

    // The pane's branch is `feature`; the space is on `main`.
    let f = fixture(|fixture| fixture.linked.clone());
    let error = review(&f.origin, &f.hosts, &space(), &cancelled).unwrap_err();
    assert!(
        matches!(&error, Error::SpaceDiverged { branch } if branch == "main"),
        "{error:?}"
    );

    let f = fixture(|fixture| fixture.main.clone());
    commit(&f.destination, "theirs.txt");
    let error = review(&f.origin, &f.hosts, &space(), &cancelled).unwrap_err();
    assert!(matches!(error, Error::SpaceDiverged { .. }), "{error:?}");
    assert_eq!(log(&f.source_home).matches("pane close").count(), 0);
}

/// A space that moved between the review and the move is left alone, and so
/// is a program that cannot start: its tab closes, the copied work stays.
#[test]
fn a_space_that_moved_since_the_review_is_left_alone() {
    let cancelled = AtomicBool::new(false);
    let f = fixture(|fixture| fixture.main.clone());
    let review_ = review(&f.origin, &f.hosts, &space(), &cancelled).unwrap();
    let theirs = commit(&f.destination, "theirs.txt");
    let error = run(&f.origin, &review_, |_| {}, &cancelled).unwrap_err();
    assert!(
        matches!(
            error,
            Error::Script {
                step: Step::Restore,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(git(&f.destination, &["rev-parse", "HEAD"]), theirs);
    assert_eq!(work_references(&f.destination), "");

    let f = fixture(|fixture| fixture.main.clone());
    fs::write(f.destination_home.join("refuse-run"), "").unwrap();
    let review_ = review(&f.origin, &f.hosts, &space(), &cancelled).unwrap();
    let error = run(&f.origin, &review_, |_| {}, &cancelled).unwrap_err();
    assert!(
        matches!(
            error,
            Error::Script {
                step: Step::Launch,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(log(&f.destination_home).contains("tab close w5:t2"));
    assert!(!log(&f.source_home).contains("pane close"));
}

/// A commit made here after the review is not what the review promised:
/// the move stops before the destination changes, rather than landing that
/// commit's work as uncommitted changes.
#[test]
fn a_commit_made_after_the_review_stops_the_move() {
    let cancelled = AtomicBool::new(false);
    let f = fixture(|fixture| fixture.main.clone());
    let head = git(&f.destination, &["rev-parse", "HEAD"]);
    let review_ = review(&f.origin, &f.hosts, &space(), &cancelled).unwrap();
    commit(&f.main, "later.txt");
    let error = run(&f.origin, &review_, |_| {}, &cancelled).unwrap_err();
    assert!(matches!(error, Error::SourceMoved), "{error:?}");
    assert_eq!(git(&f.destination, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&f.destination, &["status", "--porcelain"]), "");
    assert_eq!(work_references(&f.destination), "");
    assert!(!log(&f.source_home).contains("pane close"));
}

/// A branch another destination checkout took after the review is left
/// alone: the new worktree gets a branch of its own instead.
#[test]
fn a_branch_taken_after_the_review_is_left_alone() {
    let cancelled = AtomicBool::new(false);
    let f = fixture(|fixture| fixture.linked.clone());
    let review_ = review(&f.origin, &f.hosts, &Choice::NewWorktree, &cancelled).unwrap();
    assert_eq!(
        review_.landing,
        Landing::Worktree {
            branch: Some("feature".into())
        }
    );
    let taken = f.destination_home.join("taken");
    git(
        &f.destination,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            taken.to_str().unwrap(),
        ],
    );
    let before = git(&taken, &["rev-parse", "HEAD"]);
    run(&f.origin, &review_, |_| {}, &cancelled).unwrap();
    assert_eq!(git(&taken, &["rev-parse", "HEAD"]), before);
    assert_eq!(git(&f.destination, &["rev-parse", "feature"]), before);
    let landed = f.destination_home.join("worktree");
    assert_eq!(git(&landed, &["branch", "--show-current"]), "carried");
}

/// A device can hold two clones sharing the source's remote. A space in
/// either is checked against its own clone, whichever one a new worktree
/// would join.
#[test]
fn a_space_in_a_second_clone_is_checked_against_that_clone() {
    let cancelled = AtomicBool::new(false);
    let f = fixture(|fixture| fixture.main.clone());
    let second = f.destination_home.join("second clone");
    git(
        &f.destination,
        &[
            "clone",
            "-q",
            f.main.to_str().unwrap(),
            second.to_str().unwrap(),
        ],
    );
    git(&second, &["remote", "set-url", "origin", ORIGIN_URL]);
    let key = second.join(".git").to_string_lossy().into_owned();
    let snapshot = serde_json::json!({"result": {"snapshot": {
        "workspaces": [{"workspace_id": "w7", "worktree": {
            "repo_key": key, "repo_name": "app", "repo_root": second,
            "checkout_path": second, "is_linked_worktree": false,
        }}],
        "tabs": [], "panes": [],
    }}});
    fs::write(
        f.destination_home.join("snapshot.json"),
        snapshot.to_string(),
    )
    .unwrap();
    let chosen = Choice::Space {
        workspace_id: "w7".into(),
    };
    let review_ = review(&f.origin, &f.hosts, &chosen, &cancelled).unwrap();
    assert_eq!(review_.repository.key, key);
    let Landing::Space { checkout, .. } = &review_.landing else {
        panic!("{:?}", review_.landing);
    };
    assert_eq!(checkout, second.to_str().unwrap());
}

/// Points the source pane's working directory at `cwd`, with only its shell
/// running when `shell` is set, and `make watch` there otherwise.
fn pane_in(f: &Fixture, cwd: &Path, shell: bool) {
    let path = f.source_home.join("snapshot.json");
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    snapshot["result"]["snapshot"]["panes"][0]["foreground_cwd"] = cwd.to_str().unwrap().into();
    fs::write(&path, snapshot.to_string()).unwrap();
    let processes = if shell {
        serde_json::json!([])
    } else {
        serde_json::json!([{"pid": 42, "argv": ["make", "watch"], "cwd": cwd}])
    };
    let process = serde_json::json!({"result": {"process_info": {
        "foreground_process_group_id": if shell { 7 } else { 42 }, "shell_pid": 7,
        "foreground_processes": processes,
    }}});
    fs::write(f.source_home.join("process.json"), process.to_string()).unwrap();
}

/// A pane that changed into another repository uses that one, not its
/// space's checkout, so carrying the checkout would carry the wrong work.
#[test]
fn a_pane_outside_its_checkout_is_refused() {
    let f = fixture(|fixture| fixture.linked.clone());
    pane_in(&f, &f.main, false);
    let error = review(
        &f.origin,
        &f.hosts,
        &Choice::NewWorktree,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::OutsideCheckout { path } if path == f.main.to_str().unwrap()),
        "{error:?}"
    );
}

/// A shell has no program to restart, but still lands in its subdirectory.
#[test]
fn a_shell_lands_in_its_subdirectory() {
    let cancelled = AtomicBool::new(false);
    let f = fixture(|fixture| fixture.linked.clone());
    pane_in(&f, &f.linked.join("sub"), true);
    let review_ = review(&f.origin, &f.hosts, &Choice::NewWorktree, &cancelled).unwrap();
    assert_eq!(review_.action, Action::Shell);
    run(&f.origin, &review_, |_| {}, &cancelled).unwrap();
    let landed = f.destination_home.join("worktree").join("sub");
    let expected = format!(
        "pane run w9:p1 cd -- {}\n",
        shell_quote(landed.to_str().unwrap())
    );
    assert!(
        log(&f.destination_home).contains(&expected),
        "{}",
        log(&f.destination_home)
    );
}

/// An ignored file in the space is invisible to its clean check, so the
/// fast-forward must refuse to replace it with a committed one.
#[test]
fn a_space_keeps_its_ignored_files() {
    let cancelled = AtomicBool::new(false);
    let f = fixture(|fixture| fixture.main.clone());
    let head = git(&f.destination, &["rev-parse", "HEAD"]);
    fs::write(f.destination.join(".git/info/exclude"), "ahead.txt\n").unwrap();
    fs::write(f.destination.join("ahead.txt"), "theirs").unwrap();
    commit(&f.main, "ahead.txt");
    let review_ = review(&f.origin, &f.hosts, &space(), &cancelled).unwrap();
    let error = run(&f.origin, &review_, |_| {}, &cancelled).unwrap_err();
    assert!(
        matches!(
            error,
            Error::Script {
                step: Step::Restore,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(
        fs::read_to_string(f.destination.join("ahead.txt")).unwrap(),
        "theirs"
    );
    assert_eq!(git(&f.destination, &["rev-parse", "HEAD"]), head);
    assert!(!log(&f.source_home).contains("pane close"));
}
