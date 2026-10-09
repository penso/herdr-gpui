#![allow(clippy::unwrap_used)]
// Not a glob: the parent's `gpui::*` would shadow the `#[test]` that
// `gpui::test` expands to.
use super::{CodeSearch, HerdrWindow, Indexes, Mode, Status, rank};
use crate::code_index::{Change, Index, Kind, Symbol};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

fn symbol(name: &str, kind: Kind, file: usize, line: u32) -> Symbol {
    Symbol {
        name: name.into(),
        kind,
        file,
        line,
    }
}

fn fixture() -> Index {
    Index::of(
        Path::new("/repo"),
        &["src/code_search.rs", "src/main.rs", "src/window/render.rs"],
        vec![
            symbol("render_code_search", Kind::Method, 0, 300),
            symbol("CodeSearch", Kind::Type, 0, 60),
            symbol("main", Kind::Function, 1, 1),
            symbol("render", Kind::Method, 2, 12),
        ],
    )
}

fn labels(index: &Index, mode: Mode, query: &str) -> Vec<String> {
    rank::rank(index, mode, &rank::Query::parse(query, mode))
        .iter()
        .map(|hit| rank::label(index, mode, hit.item).to_owned())
        .collect()
}

#[test]
fn symbols_rank_by_match_then_by_shorter_name() {
    let index = fixture();
    assert_eq!(
        labels(&index, Mode::Symbols, "render"),
        ["render", "render_code_search"]
    );
    assert_eq!(
        labels(&index, Mode::Symbols, "cdsrch"),
        ["CodeSearch", "render_code_search"]
    );
    // Without a query every symbol lists, in index order.
    assert_eq!(labels(&index, Mode::Symbols, "  ").len(), 4);
    assert!(labels(&index, Mode::Symbols, "zzz").is_empty());
    let hits = rank::rank(
        &index,
        Mode::Symbols,
        &rank::Query::parse("main", Mode::Symbols),
    );
    assert_eq!(hits[0].highlights.len(), 1);
    assert_eq!(hits[0].highlights[0], 0..4);
}

#[test]
fn files_match_paths_and_take_a_line() {
    let index = fixture();
    assert_eq!(
        labels(&index, Mode::Files, "render"),
        ["src/window/render.rs"]
    );
    let query = rank::Query::parse("main.rs:42", Mode::Files);
    assert_eq!((query.text.as_str(), query.line), ("main.rs", Some(42)));
    assert_eq!(labels(&index, Mode::Files, "main.rs:42"), ["src/main.rs"]);
    // A symbol query keeps its colon.
    assert_eq!(rank::Query::parse("a:1", Mode::Symbols).line, None);
}

#[test]
fn without_a_query_the_changed_files_come_first() {
    let change = |file: usize| Change {
        file,
        counts: Some((1, 1)),
        new: false,
        modified: None,
    };
    // `src/window/render.rs` changed last, then `src/code_search.rs`.
    let index = fixture().with_changes(vec![change(2), change(0)]);
    assert_eq!(
        labels(&index, Mode::Files, ""),
        ["src/window/render.rs", "src/code_search.rs", "src/main.rs"]
    );
    assert_eq!(
        labels(&index, Mode::Symbols, ""),
        ["render", "render_code_search", "CodeSearch", "main"]
    );
    // A query ranks by match alone.
    assert_eq!(labels(&index, Mode::Files, "main"), ["src/main.rs"]);
}

#[test]
fn hits_are_bounded() {
    let names: Vec<String> = (0..rank::MAX_HITS + 50)
        .map(|i| format!("item{i}"))
        .collect();
    let files: Vec<&str> = names.iter().map(String::as_str).collect();
    let index = Index::of(Path::new("/repo"), &files, Vec::new());
    for query in ["", "item"] {
        assert_eq!(labels(&index, Mode::Files, query).len(), rank::MAX_HITS);
    }
}

#[test]
fn a_few_recent_checkouts_are_kept() {
    let mut indexes = Indexes::default();
    let now = Instant::now();
    for i in 0..6 {
        let root = PathBuf::from(format!("/repo{i}"));
        indexes.insert(Arc::new(Index::of(&root, &[], Vec::new())), now);
    }
    // Searching one again keeps it, newest first.
    indexes.insert(
        Arc::new(Index::of(Path::new("/repo3"), &["a"], Vec::new())),
        now,
    );
    assert!(indexes.get(Path::new("/repo0")).is_none());
    assert_eq!(indexes.get(Path::new("/repo3")).unwrap().0.files(), ["a"]);
    assert_eq!(indexes.entries.len(), super::CACHED);
}

/// A window whose focused pane is in `cwd`.
fn window<'a>(
    cx: &'a mut TestAppContext,
    cwd: Option<&Path>,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let cwd = cwd.map(|cwd| cwd.to_str().unwrap().to_owned());
    cx.add_window_view(move |window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut shown = serde_json::to_value(crate::sidebar::layout_tests::snapshot(1)).unwrap();
        shown["focused_workspace_id"] = "w0".into();
        shown["focused_pane_id"] = "w0:p1".into();
        shown["panes"] = serde_json::json!([{
            "pane_id": "w0:p1", "workspace_id": "w0", "tab_id": "t0", "label": null,
            "cwd": cwd, "foreground_cwd": null, "focused": true,
            "right_click_passthrough": false
        }]);
        view.live.snapshot = Some(Arc::new(serde_json::from_value(shown).unwrap()));
        view
    })
}

fn checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(root)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn alpha() {}\n\npub struct Beta;\n",
    )
    .unwrap();
    dir
}

fn search<R>(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    read: impl FnOnce(&CodeSearch) -> R,
) -> R {
    view.read_with(cx, |view, _| read(view.menu.code_search.as_ref().unwrap()))
}

#[gpui::test]
fn a_symbol_opens_in_the_editor_or_a_code_tab(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx, Some(&dir.path().join("src")));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_code_search(Mode::Symbols, window, cx)
        })
    });
    cx.run_until_parked();
    search(&view, cx, |code| {
        assert_eq!(code.status, Status::Ready);
        assert_eq!(code.beside.as_deref(), Some("w0:p1"));
        assert_eq!(code.hits.len(), 2);
    });
    // The checkout's one file is new, and its rows say so.
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("code-search-row-1").is_some());
    cx.simulate_input("beta");
    cx.run_until_parked();
    let target = search(&view, cx, |code| {
        assert_eq!(code.hits.len(), 1);
        code.target(&code.hits[0]).unwrap()
    });
    assert_eq!(target.line, Some(3));
    assert!(target.path.ends_with("src/lib.rs"));

    // Enter opens the editor; this fixture has no connection to split.
    // Without a Unix shell for an editor, a code tab opens instead.
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.menu.code_search.is_none());
        if crate::editor::SUPPORTED {
            let (flash, _) = view.flash.as_ref().unwrap();
            assert_eq!(flash.text.as_ref(), crate::Error::NotConnected.to_string());
        } else {
            assert_eq!(view.code_views.len(), 1);
        }
    });

    // The checkout is kept, so the next search is ready at once; the
    // secondary modifier opens a code tab at the symbol's line instead.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_code_search(Mode::Symbols, window, cx)
        })
    });
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.activate_code_search(0, true, window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.menu.code_search.is_none());
        let code = view.code_views.values().next().unwrap();
        assert_eq!(code.shown(), (Some(1), Some(3)));
    });
}

#[gpui::test]
fn files_mode_lists_the_checkout_and_tab_switches(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx, Some(dir.path()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_code_search(Mode::Symbols, window, cx)
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("tab");
    cx.simulate_input("lib:2");
    cx.run_until_parked();
    let target = search(&view, cx, |code| {
        assert_eq!(code.mode, Mode::Files);
        assert_eq!(code.hits.len(), 1);
        code.target(&code.hits[0]).unwrap()
    });
    assert_eq!(target.line, Some(2));
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.menu.code_search.is_none()));
}

#[gpui::test]
fn without_a_local_checkout_the_search_says_so(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_code_search(Mode::Files, window, cx)
        })
    });
    cx.run_until_parked();
    search(&view, cx, |code| {
        assert_eq!(
            code.status,
            Status::Failed(crate::Error::CodeIndexRoot.to_string())
        );
    });
}

#[gpui::test]
fn outside_a_checkout_the_search_shows_why(cx: &mut TestAppContext) {
    let outside = tempfile::tempdir().unwrap();
    let (view, cx) = window(cx, Some(outside.path()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_code_search(Mode::Files, window, cx)
        })
    });
    cx.run_until_parked();
    search(&view, cx, |code| {
        assert!(
            matches!(code.status, Status::Failed(_)),
            "{:?}",
            code.status
        );
    });
}

#[gpui::test]
fn shown_hits_keep_the_index_they_were_ranked_against(cx: &mut TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx, Some(dir.path()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_code_search(Mode::Files, window, cx)
        })
    });
    cx.run_until_parked();
    let opens = |view: &Entity<HerdrWindow>, cx: &mut VisualTestContext| {
        search(view, cx, |code| code.target(&code.hits[0]).unwrap().path)
    };
    assert!(opens(&view, cx).ends_with("src/lib.rs"));
    // A refreshed index lists other files at the same item numbers; until
    // its ranking lands, the rows shown still open what they say.
    let root = search(&view, cx, |code| {
        code.index.as_ref().unwrap().root().to_owned()
    });
    let refreshed = Arc::new(Index::of(
        &root,
        &["a.rs", "b.rs", "src/lib.rs"],
        Vec::new(),
    ));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let token = view.menu.code_search.as_ref().unwrap().search.clone();
            view.checkout_indexed(&token, Ok(refreshed), cx);
        })
    });
    assert!(opens(&view, cx).ends_with("src/lib.rs"));
    cx.run_until_parked();
    assert!(opens(&view, cx).ends_with("a.rs"));
    // The row the user was on stays selected where it moved to.
    let selected = search(&view, cx, |code| {
        code.target(&code.hits[code.selected]).unwrap().path
    });
    assert!(selected.ends_with("src/lib.rs"));
}
