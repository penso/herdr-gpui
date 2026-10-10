use super::*;
use crate::orchestrator::store::SCHEMA_V0;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

fn database(dir: &tempfile::TempDir) -> PathBuf {
    dir.path()
        .join("repositories")
        .join("id")
        .join("state.sqlite3")
}

fn pragma(path: &Path, name: &str) -> String {
    Connection::open(path)
        .unwrap()
        .query_row(&format!("PRAGMA {name}"), [], |row| {
            row.get::<_, rusqlite::types::Value>(0)
        })
        .map(|value| match value {
            rusqlite::types::Value::Integer(value) => value.to_string(),
            rusqlite::types::Value::Text(value) => value,
            other => format!("{other:?}"),
        })
        .unwrap()
}

/// A database as agent-launcher wrote it before the contract: version 0,
/// without `runs.owner` or `herdr_sessions`.
fn version_zero(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let connection = Connection::open(path).unwrap();
    connection.execute_batch(SCHEMA_V0).unwrap();
    connection
        .execute_batch(
            "INSERT INTO runs (id, issue_key, workspace_json, agent, model, state_json, message, \
             session_id, started_at, updated_at) VALUES ('92f7ebf5-f03d-41db-83fc-7250437cf852', \
             'github:github.com:penso/herdr-gpui:340', \
             '{\"backend\":\"herdr\",\"id\":\"w95\",\"host\":null,\"path\":\"/tmp/wt\",\"branch\":\"agent/x\"}', \
             'opencode', NULL, '\"completed\"', NULL, 'launcher-b8e1e78af67f44e8a209cfe', \
             '2026-10-08T06:12:44.896730+00:00', '2026-10-08T06:12:47.451152+00:00');
             INSERT INTO issues (canonical_key, source, provider, host, repository, native_id, \
             identifier, title, state, labels_json, blocked_by_json, created_at, updated_at, \
             activity_json) VALUES ('github:github.com:penso/herdr-gpui:368', \
             'github:github.com:penso/herdr-gpui', 'github', 'github.com', 'penso/herdr-gpui', \
             '368', '#368', 'Teleport fails', 'open', '[\"enhancement\"]', '[]', \
             '2026-10-08T23:23:37+00:00', '2026-10-08T23:23:37+00:00', \
             '{\"comments\":0,\"review_comments\":null,\"commits\":null}');",
        )
        .unwrap();
    private(path);
}

/// The modes agent-launcher gives its database and directory.
fn private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path, mode| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        mode(path.parent().unwrap(), 0o700);
        mode(path, 0o600);
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[test]
fn a_new_database_is_private_versioned_and_in_wal_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.access(), Access::ReadWrite);
    assert_eq!(pragma(&path, "user_version"), SCHEMA_VERSION.to_string());
    assert_eq!(pragma(&path, "journal_mode"), "wal");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
    }
}

#[test]
fn version_zero_migrates_once_and_keeps_agent_launcher_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    version_zero(&path);
    for _ in 0..2 {
        let store = Store::open(&path).unwrap();
        let runs = store.runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].owner, Owner::AgentLauncher);
        assert_eq!(runs[0].state, RunState::Completed);
        assert_eq!(runs[0].workspace.as_ref().unwrap().id, "w95");
        let items = store.items().unwrap();
        assert_eq!(items[0].identifier, "#368");
        assert_eq!(items[0].labels, ["enhancement"]);
        assert_eq!(items[0].activity.unwrap().comments, Some(0));
        assert!(store.sessions().unwrap().is_empty());
    }
    assert_eq!(pragma(&path, "user_version"), "1");
}

#[test]
fn a_newer_schema_opens_read_only_and_refuses_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    drop(Store::open(&path).unwrap());
    Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version = 2")
        .unwrap();
    let mut store = Store::open(&path).unwrap();
    assert_eq!(store.access(), Access::ReadOnly { found: 2 });
    assert!(store.runs().unwrap().is_empty());
    let error = store
        .save_run(&run("a", Owner::HerdrGpui), None)
        .unwrap_err();
    assert!(matches!(error, Error::NewerSchema { found: 2, known: 1 }));
    let error = store
        .replace_items(&github(), &[], &Checkpoint::default())
        .unwrap_err();
    assert!(matches!(error, Error::NewerSchema { .. }));
}

#[test]
fn own_runs_round_trip_with_their_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&database(&dir)).unwrap();
    let mut saved = run("4f7c", Owner::HerdrGpui);
    store.save_run(&saved, Some(&session("4f7c"))).unwrap();
    assert_eq!(store.runs().unwrap(), [saved.clone()]);
    assert_eq!(store.sessions().unwrap(), [session("4f7c")]);

    saved.state = RunState::NeedsInput;
    store.save_run(&saved, None).unwrap();
    assert_eq!(store.runs().unwrap()[0].state, RunState::NeedsInput);
    assert!(store.sessions().unwrap().is_empty());

    store.delete_run("4f7c").unwrap();
    assert!(store.runs().unwrap().is_empty());
    assert!(matches!(
        store.delete_run("4f7c"),
        Err(Error::RunNotFound(id)) if id == "4f7c"
    ));
    // An action that read the run before it was removed cannot bring it,
    // or its session, back.
    assert!(matches!(
        store.update_run(&saved, Some(&session("4f7c"))),
        Err(Error::RunNotFound(id)) if id == "4f7c"
    ));
    assert!(store.runs().unwrap().is_empty());
    assert!(store.sessions().unwrap().is_empty());
}

#[test]
fn agent_launcher_runs_are_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    version_zero(&path);
    let mut store = Store::open(&path).unwrap();
    let id = "92f7ebf5-f03d-41db-83fc-7250437cf852";
    let theirs = store.runs().unwrap().remove(0);

    // Claiming their id as our own is refused, as is writing their row.
    let claimed = Run {
        owner: Owner::HerdrGpui,
        ..theirs.clone()
    };
    assert!(matches!(store.save_run(&claimed, None), Err(Error::NotOwner(run)) if run == id));
    assert!(matches!(
        store.save_run(&theirs, None),
        Err(Error::NotOwner(_))
    ));
    assert!(matches!(store.delete_run(id), Err(Error::NotOwner(_))));
    assert_eq!(store.runs().unwrap(), [theirs]);
}

#[test]
fn a_full_sync_replaces_only_its_source() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&database(&dir)).unwrap();
    let mut bead = item(&beads(), "hg-a3f2", "Epic");
    bead.priority = Some(1);
    bead.parent_id = None;
    bead.blocked_by = vec!["hg-a3f2.1".into()];
    store
        .replace_items(
            &beads(),
            std::slice::from_ref(&bead),
            &Checkpoint::default(),
        )
        .unwrap();
    let mut pr = item(&github(), "pr/369", "Find bar");
    pr.pull_request = Some(PullRequest {
        number: 369,
        additions: Some(312),
        deletions: Some(40),
        base_ref: "main".into(),
        head_ref: "feat/pane-find-memory".into(),
        base_sha: "a".repeat(40),
        head_sha: "b".repeat(40),
        head_repository: Some("penso/herdr-gpui".into()),
    });
    let checkpoint = Checkpoint {
        last_full_at: Some(at("2026-10-09T02:24:37Z")),
        ..Checkpoint::default()
    };
    let first = [item(&github(), "1", "Old"), pr.clone()];
    store.replace_items(&github(), &first, &checkpoint).unwrap();
    store
        .replace_items(&github(), std::slice::from_ref(&pr), &checkpoint)
        .unwrap();

    let mut items = store.items().unwrap();
    items.sort_by_key(|item| item.key.canonical());
    assert_eq!(items, [bead, pr]);
    assert_eq!(store.checkpoint(&github()).unwrap(), Some(checkpoint));
    assert_eq!(
        store.checkpoint(&beads()).unwrap(),
        Some(Checkpoint::default())
    );
}

#[test]
#[cfg(unix)]
fn a_linked_database_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let elsewhere = dir.path().join("elsewhere.sqlite3");
    std::fs::write(&elsewhere, b"").unwrap();
    std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
    assert!(matches!(Store::open(&path), Err(Error::NotPrivate { .. })));
}

/// An existing database others could read is refused rather than written,
/// as is one in a directory others could list; private ones, as
/// agent-launcher makes them, open.
#[test]
#[cfg(unix)]
fn a_database_others_can_read_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path, mode| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let folder = path.parent().unwrap().to_owned();
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(&path, b"").unwrap();

    mode(&folder, 0o700);
    mode(&path, 0o644);
    assert!(matches!(Store::open(&path), Err(Error::NotPrivate { .. })));
    mode(&path, 0o600);
    mode(&folder, 0o755);
    assert!(matches!(Store::open(&path), Err(Error::NotPrivate { .. })));
    mode(&folder, 0o700);
    assert!(Store::open(&path).is_ok());
}
