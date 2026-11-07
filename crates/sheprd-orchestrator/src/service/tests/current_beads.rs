//! Beads a project still stores under a source it no longer reads.
use super::super::sources::keep_current_beads;
use super::opened;
use crate::{
    Checkpoint, Item, ItemKey, Owner, Provider, Repository, Run, RunState, SourceKey,
    repo::{Extras, RepoInfo},
};
use chrono::Utc;

fn bead(root: &str) -> Item {
    Item::stand_in(ItemKey {
        source: SourceKey {
            provider: Provider::Beads,
            host: "local".into(),
            repository: root.into(),
        },
        native_id: "tt-1".into(),
    })
}

fn issue() -> Item {
    Item::stand_in(ItemKey {
        source: SourceKey {
            provider: Provider::Github,
            host: "github.com".into(),
            repository: "owner/app".into(),
        },
        native_id: "1".into(),
    })
}

fn info(beads: bool, beads_root: Option<&str>) -> RepoInfo {
    RepoInfo {
        repository: Repository::Local("/app/.git".into()),
        main_root: "/app".into(),
        beads,
        beads_root: beads_root.map(str::to_owned),
        remote: None,
    }
}

fn roots(items: &[Item]) -> Vec<&str> {
    items
        .iter()
        .map(|item| item.key.source.repository.as_str())
        .collect()
}

#[test]
fn a_redirect_added_later_leaves_only_the_database_it_names() {
    let mut items = vec![bead("/app"), bead("/shared"), issue()];
    keep_current_beads(&mut items, Some(&info(true, Some("/shared"))));
    assert_eq!(roots(&items), ["/shared", "owner/app"]);
}

#[test]
fn a_project_without_beads_keeps_its_other_items_only() {
    let mut items = vec![bead("/app"), issue()];
    keep_current_beads(&mut items, Some(&info(false, None)));
    assert_eq!(roots(&items), ["owner/app"]);
}

#[test]
fn what_was_listed_stays_until_the_repository_is_known() {
    let mut items = vec![bead("/app"), issue()];
    keep_current_beads(&mut items, None);
    assert_eq!(items.len(), 2);
}

/// A worker that has listed a bead under its project's own source, with a
/// run on it, as a redirected project has from an earlier build.
fn listed(dir: &std::path::Path) -> (super::Service, super::Worker, super::Host, RepoInfo) {
    let (service, mut worker, host) = opened(dir);
    let mut info = worker.snapshot.repo.clone().unwrap();
    info.beads = true;
    let old = bead(&info.main_root);
    let run = Run {
        id: "r1".into(),
        item_key: old.key.canonical(),
        workspace: None,
        agent: "claude".into(),
        model: None,
        state: RunState::Running,
        message: None,
        session_id: None,
        started_at: Utc::now(),
        updated_at: Utc::now(),
        owner: Owner::Sheprd,
    };
    let store = worker.store.as_mut().unwrap();
    store
        .replace_items(
            &old.key.source,
            std::slice::from_ref(&old),
            &Checkpoint::default(),
        )
        .unwrap();
    store.save_run(&run, None).unwrap();
    (service, worker, host, info)
}

#[test]
fn a_cached_repository_settles_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (_service, mut worker, host, mut cached) = listed(dir.path());
    cached.beads_root = Some("/shared".into());
    worker.show(cached, &host).unwrap();
    let store = worker.store.as_ref().unwrap();
    assert_eq!(store.items().unwrap().len(), 1);
    assert!(store.runs().unwrap()[0].item_key.contains("app"));
}

#[test]
fn discovery_moves_the_source_and_shows_the_moved_runs() {
    let dir = tempfile::tempdir().unwrap();
    let (_service, mut worker, host, mut info) = listed(dir.path());
    info.beads_root = Some("/shared".into());
    worker.found(&host, Ok((info, Extras::default()))).unwrap();
    assert!(worker.store.as_ref().unwrap().items().unwrap().is_empty());
    assert_eq!(
        worker.snapshot.runs[0].item_key,
        bead("/shared").key.canonical()
    );
}

#[test]
fn discovery_settles_even_when_it_finds_what_the_cache_showed() {
    let dir = tempfile::tempdir().unwrap();
    let (_service, mut worker, host, mut info) = listed(dir.path());
    info.beads_root = Some("/shared".into());
    worker.show(info.clone(), &host).unwrap();
    worker.found(&host, Ok((info, Extras::default()))).unwrap();
    assert_eq!(
        worker.snapshot.runs[0].item_key,
        bead("/shared").key.canonical()
    );
}
