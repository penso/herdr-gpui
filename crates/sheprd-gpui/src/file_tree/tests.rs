#![allow(clippy::unwrap_used)]
// Not a glob: the parent's `gpui::*` would shadow the `#[test]` that
// `gpui::test` expands to.
use super::{Change, Entry, FileTree, Listing, Node, Prepared, REREAD, matches};
use crate::{
    SheprdWindow,
    browser::{Location, Store, TabId, scope},
    git::{Git, Status},
    pull_request::Input,
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{path::Path, process::Command, sync::Arc, time::Instant};

mod en_xa;

fn entry(path: &str, change: Option<Change>) -> Entry {
    Entry {
        path: path.into(),
        change,
    }
}

fn prepared(entries: Vec<Entry>) -> Arc<Prepared> {
    Arc::new(Prepared::of(Listing::fixture(Path::new("/r"), entries)))
}

/// The shown rows' folder paths and file paths.
fn rows(view: &FileTree) -> Vec<String> {
    let prepared = view.prepared.as_ref().unwrap();
    view.shown
        .iter()
        .map(|node| prepared.key(*node).unwrap().to_owned())
        .collect()
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A checkout with `src/main.rs` changed, `src/gone.rs` deleted, and
/// `notes/new.txt` untracked.
fn checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("src/gone.rs"), "gone\n").unwrap();
    std::fs::write(root.join("docs/guide.md"), "guide\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "first"]);
    std::fs::write(root.join("src/main.rs"), "fn main() { todo!() }\n").unwrap();
    std::fs::remove_file(root.join("src/gone.rs")).unwrap();
    std::fs::write(root.join("notes/new.txt"), "new\n").unwrap();
    dir
}

fn window(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut shown = serde_json::to_value(crate::sidebar::layout_tests::snapshot(1)).unwrap();
        shown["focused_workspace_id"] = "w0".into();
        view.live.snapshot = Some(Arc::new(serde_json::from_value(shown).unwrap()));
        view
    })
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

/// Opens `root`'s file tree, its listing read and drawn. Its tab.
fn open(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, root: &Path) -> TabId {
    let input = Input {
        checkout: Some(root.to_str().unwrap().into()),
        repo_key: None,
        branch: "main".into(),
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.git = Git::fixture(input, Status::default());
            view.open_file_tree(window, cx);
        })
    });
    cx.run_until_parked();
    draw(cx);
    view.read_with(cx, |view, _| *view.file_trees.keys().next().unwrap())
}

fn read<R>(
    view: &Entity<SheprdWindow>,
    cx: &VisualTestContext,
    read: impl FnOnce(&FileTree) -> R,
) -> R {
    view.read_with(cx, |view, _| read(view.file_trees.values().next().unwrap()))
}

#[test]
fn saved_file_tree_tabs_are_checked_when_read() {
    let absolute = std::env::temp_dir();
    let saved = |path: &str| {
        serde_json::json!({
            "kind": "files",
            "checkout": {"branch": "main", "checkout": path},
        })
        .to_string()
    };
    let location: Location = serde_json::from_str(&saved(absolute.to_str().unwrap())).unwrap();
    assert!(!location.is_page());
    assert_eq!(location.default_title(), "Files \u{00b7} main");
    assert!(serde_json::from_str::<Location>(&saved("relative")).is_err());
}

#[test]
fn a_folder_is_marked_when_it_holds_a_change_at_any_depth() {
    let prepared = prepared(vec![
        entry("a/deep/er/file.rs", Some(Change::Modified)),
        entry("a/other.rs", None),
        entry("b/kept.rs", None),
    ]);
    let marked: Vec<&str> = prepared
        .folders()
        .filter(|(node, _)| prepared.changed[*node])
        .map(|(_, path)| path.as_str())
        .collect();
    assert_eq!(marked, ["a/", "a/deep/er/"]);
}

#[test]
fn the_filter_matches_paths_loosely_and_nothing_means_no_filter() {
    let listing = Listing::fixture(
        Path::new("/r"),
        vec![
            entry("src/review/view.rs", None),
            entry("docs/cli.md", None),
        ],
    );
    assert_eq!(matches(&listing, "  "), None);
    assert_eq!(matches(&listing, "rvw"), Some(vec![true, false]));
    assert_eq!(matches(&listing, "cli"), Some(vec![false, true]));
    assert_eq!(matches(&listing, "zzz"), Some(vec![false, false]));
}

#[gpui::test]
fn folders_start_closed_unless_they_hold_a_change_and_choices_survive_a_read(
    cx: &mut TestAppContext,
) {
    let dir = checkout();
    let (view, cx) = window(cx);
    let id = open(&view, cx, dir.path());
    read(&view, cx, |tree| {
        assert_eq!(
            rows(tree),
            [
                "docs/",
                "notes/",
                "notes/new.txt",
                "src/",
                "src/gone.rs",
                "src/main.rs"
            ]
        );
        assert_eq!(tree.file_count(), 4);
    });
    // Closing a changed folder lasts through another read of the checkout.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let tree = view.file_trees.get_mut(&id).unwrap();
            let node = tree.shown[3];
            tree.toggle_folder(node);
            tree.picked = Some(1);
            view.load_file_tree(id, cx);
        })
    });
    cx.run_until_parked();
    read(&view, cx, |tree| {
        assert_eq!(rows(tree), ["docs/", "notes/", "notes/new.txt", "src/"]);
        assert_eq!(tree.picked, Some(1), "the pick stays on its row");
    });
}

#[gpui::test]
fn each_file_shows_its_change_and_a_folder_shows_it_holds_one(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx);
    open(&view, cx, dir.path());
    assert!(cx.debug_bounds("file-tree-change-2-U").is_some());
    assert!(cx.debug_bounds("file-tree-change-4-D").is_some());
    assert!(cx.debug_bounds("file-tree-change-5-M").is_some());
    assert!(cx.debug_bounds("file-tree-folder-changed-1").is_some());
    assert!(
        cx.debug_bounds("file-tree-folder-changed-0").is_none(),
        "docs holds no change"
    );
}

#[gpui::test]
fn typing_filters_the_tree_and_enter_opens_a_code_tab(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx);
    open(&view, cx, dir.path());
    cx.simulate_input("guide");
    cx.run_until_parked();
    read(&view, cx, |tree| {
        // A closed folder opens for its match.
        assert_eq!(rows(tree), ["docs/", "docs/guide.md"]);
        assert_eq!(tree.picked, Some(1), "the first file, not its folder");
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.code_views.len(), 1);
        let (scope, workspace) = view.browser_key().unwrap();
        let code = cx
            .global::<Store>()
            .in_workspace(&scope, &workspace)
            .find_map(|tab| match &tab.location {
                Some(Location::Code { file }) => Some(file.path.clone()),
                _ => None,
            })
            .unwrap();
        assert!(code.ends_with("docs/guide.md"), "{code}");
    });
}

#[gpui::test]
fn escape_clears_the_filter_and_a_miss_says_so(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx);
    open(&view, cx, dir.path());
    cx.simulate_input("zzzz");
    cx.run_until_parked();
    read(&view, cx, |tree| assert!(tree.shown.is_empty()));
    draw(cx);
    assert!(cx.debug_bounds("file-tree-row-0").is_none());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    read(&view, cx, |tree| {
        assert!(tree.matched.is_none());
        assert_eq!(tree.shown.len(), 6);
    });
}

#[gpui::test]
fn the_arrows_walk_rows_and_left_and_right_fold_folders(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx);
    open(&view, cx, dir.path());
    cx.simulate_keystrokes("down");
    read(&view, cx, |tree| assert_eq!(tree.picked, Some(0)));
    cx.simulate_keystrokes("right");
    read(&view, cx, |tree| {
        assert_eq!(rows(tree)[..2], ["docs/", "docs/guide.md"]);
    });
    cx.simulate_keystrokes("left");
    read(&view, cx, |tree| assert_eq!(rows(tree)[1], "notes/"));
    cx.simulate_keystrokes("up");
    read(&view, cx, |tree| {
        assert_eq!(tree.picked, Some(tree.shown.len() - 1), "wraps to the end");
    });
}

#[gpui::test]
fn a_deleted_file_has_nothing_to_open(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx);
    let id = open(&view, cx, dir.path());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.activate_file_tree_row(id, 4, window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.code_views.is_empty()));
}

#[gpui::test]
fn typing_on_the_tab_itself_goes_to_the_filter(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx);
    let id = open(&view, cx, dir.path());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let focus = view.file_trees[&id].focus.clone();
            window.focus(&focus, cx);
        })
    });
    draw(cx);
    cx.simulate_keystrokes("g");
    cx.run_until_parked();
    read(&view, cx, |tree| assert!(tree.matched.is_some()));
    cx.update(|_, cx| {
        let text = view.read(cx).file_trees[&id]
            .filter
            .read(cx)
            .text()
            .to_owned();
        assert_eq!(text, "g");
    });
}

#[gpui::test]
fn a_checkout_whose_changes_moved_is_read_again(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx);
    let id = open(&view, cx, dir.path());
    std::fs::write(dir.path().join("added.txt"), "x\n").unwrap();
    let input = view.read_with(cx, |view, _| view.git.tracked().cloned().unwrap());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let load = view.file_trees[&id].load;
            // The same status reads nothing, even on another tick.
            view.poll_file_trees(cx);
            assert_eq!(view.file_trees[&id].load, load);
            let moved = Status {
                untracked: 9,
                ..Status::default()
            };
            view.git = Git::fixture(input, moved);
            view.poll_file_trees(cx);
            assert_eq!(view.file_trees[&id].load, load + 1);
        })
    });
    cx.run_until_parked();
    read(&view, cx, |tree| {
        assert!(rows(tree).contains(&"added.txt".to_owned()));
    });
}

#[gpui::test]
fn a_listing_left_alone_is_read_again_though_the_status_stands(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx);
    let id = open(&view, cx, dir.path());
    // A second file in an untracked folder the status already counts.
    std::fs::write(dir.path().join("notes/more.txt"), "x\n").unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let tree = view.file_trees.get_mut(&id).unwrap();
            tree.read_at = Instant::now().checked_sub(REREAD);
            view.poll_file_trees(cx);
        })
    });
    cx.run_until_parked();
    read(&view, cx, |tree| {
        let prepared = tree.prepared.as_ref().unwrap();
        assert!(
            prepared
                .listing
                .entries()
                .iter()
                .any(|entry| entry.path == "notes/more.txt")
        );
    });
}

#[gpui::test]
fn no_checkout_to_list_says_so(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_file_tree(window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.file_trees.is_empty());
        assert!(view.flash.is_some());
    });
}

#[gpui::test]
fn a_folder_outside_git_shows_why(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = window(cx);
    open(&view, cx, dir.path());
    assert!(cx.debug_bounds("file-tree-error").is_some());
}

#[gpui::test]
fn a_restored_tab_reads_its_listing_and_a_closed_one_is_forgotten(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx);
    let id = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        let checkout = crate::browser::ReviewCheckout {
            repo_key: None,
            branch: "main".into(),
            checkout: Some(dir.path().to_str().unwrap().into()),
        };
        Store::update(cx, |store| {
            store.open(tab_scope, "w0", Some(Location::Files { checkout }), None)
        })
        .unwrap()
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_file_trees(cx)));
    cx.run_until_parked();
    read(&view, cx, |tree| {
        assert!(tree.prepared.is_some());
        let kinds = tree.prepared.as_ref().unwrap().tree.nodes.iter();
        assert!(
            kinds
                .into_iter()
                .any(|node| matches!(node, Node::File { .. }))
        );
    });
    cx.update(|_, cx| {
        Store::update(cx, |store| store.close(id));
        view.update(cx, |view, cx| {
            view.poll_file_trees(cx);
            assert!(view.file_trees.is_empty());
        })
    });
}
