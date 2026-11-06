//! A project's Beads source moving: what stays on disk, and where runs go.
use super::{store::database, *};
use crate::{Checkpoint, Store};

fn source(root: &str) -> SourceKey {
    SourceKey {
        provider: Provider::Beads,
        host: "local".into(),
        repository: root.into(),
    }
}

fn bead_run(id: &str, root: &str) -> Run {
    let mut run = run(id, Owner::Sheprd);
    run.item_key = item(&source(root), "tt-1", "A task").key.canonical();
    run
}

/// A store whose `/app` source lists a bead, beside a GitHub issue.
fn stored(dir: &tempfile::TempDir) -> Store {
    let mut store = Store::open(&database(dir)).unwrap();
    for key in [source("/app"), github()] {
        let items = [item(&key, "tt-1", "A task")];
        store
            .replace_items(&key, &items, &Checkpoint::default())
            .unwrap();
    }
    store
}

fn sources(store: &Store) -> Vec<String> {
    let mut sources: Vec<_> = store
        .items()
        .unwrap()
        .iter()
        .map(|item| item.key.source.repository.clone())
        .collect();
    sources.sort();
    sources
}

#[test]
fn a_moved_source_leaves_nothing_old_on_disk_for_any_reader() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = stored(&dir);
    store.settle_beads(Some(&source("/shared"))).unwrap();
    assert_eq!(sources(&store), ["sheprd-labs/sheprd"]);
    assert_eq!(store.checkpoint(&source("/app")).unwrap(), None);
    assert!(store.checkpoint(&github()).unwrap().is_some());
}

#[test]
fn the_current_source_keeps_its_tasks_and_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = stored(&dir);
    store.settle_beads(Some(&source("/app"))).unwrap();
    assert_eq!(sources(&store), ["/app", "sheprd-labs/sheprd"]);
    assert!(store.checkpoint(&source("/app")).unwrap().is_some());
}

#[test]
fn a_task_keeps_its_runs_when_its_source_moves() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = stored(&dir);
    store.save_run(&bead_run("r1", "/app"), None).unwrap();
    store.settle_beads(Some(&source("/shared"))).unwrap();
    assert_eq!(
        store.runs().unwrap()[0].item_key,
        item(&source("/shared"), "tt-1", "A task").key.canonical()
    );
}

#[test]
fn only_this_apps_bead_runs_move() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let mut store = stored(&dir);
    store.save_run(&run("issue", Owner::Sheprd), None).unwrap();
    let foreign = bead_run("theirs", "/app").item_key;
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO runs (id, issue_key, agent, state_json, started_at, updated_at) VALUES \
             ('theirs', ?1, 'opencode', '\"running\"', '2026-10-08T00:00:00+00:00', \
             '2026-10-08T00:00:00+00:00')",
            [&foreign],
        )
        .unwrap();
    store.settle_beads(Some(&source("/shared"))).unwrap();
    let key = |id: &str| {
        let runs = store.runs().unwrap();
        let found = runs.iter().find(|run| run.id == id).unwrap();
        found.item_key.clone()
    };
    assert_eq!(key("theirs"), foreign);
    assert_eq!(
        key("issue"),
        item(&github(), "377", "Icons").key.canonical()
    );
}

#[test]
fn a_project_without_beads_forgets_its_tasks_and_keeps_its_runs() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = stored(&dir);
    let saved = bead_run("r1", "/app");
    store.save_run(&saved, None).unwrap();
    store.settle_beads(None).unwrap();
    assert_eq!(sources(&store), ["sheprd-labs/sheprd"]);
    assert_eq!(store.runs().unwrap()[0].item_key, saved.item_key);
}

#[test]
fn an_action_in_flight_cannot_undo_a_moved_run() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = stored(&dir);
    let mut read_before = bead_run("r1", "/app");
    store.save_run(&read_before, None).unwrap();
    store.settle_beads(Some(&source("/shared"))).unwrap();
    read_before.state = RunState::Cancelled;
    store.update_run(&read_before, None).unwrap();
    let saved = store.runs().unwrap().remove(0);
    assert_eq!(saved.state, RunState::Cancelled);
    assert_eq!(saved.item_key, bead_run("r1", "/shared").item_key);
}
