//! Beads a project still stores under a source it no longer reads.
use super::super::sources::{keep_current_beads, settle_beads};
use crate::{Checkpoint, Item, ItemKey, Provider, Repository, SourceKey, Store, repo::RepoInfo};

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

#[test]
fn finding_the_repository_settles_what_the_store_holds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repositories/id/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let old = bead("/app");
    store
        .replace_items(
            &old.key.source,
            std::slice::from_ref(&old),
            &Checkpoint::default(),
        )
        .unwrap();
    settle_beads(&mut store, &info(true, Some("/shared")));
    assert!(store.items().unwrap().is_empty());
}
