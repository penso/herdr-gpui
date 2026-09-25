#![allow(clippy::unwrap_used)]

use super::{
    Page, WorkspaceAction,
    worktree_source::{Pending, Tab, item_request, pick_request},
};
use crate::{
    HerdrWindow,
    repo_items::{Branch, Item, Kind, Origin},
    sidebar,
};
use gpui::{Entity, VisualTestContext};

fn origin() -> Origin {
    Origin {
        owner: "penso".into(),
        repo: "herdr-gpui".into(),
    }
}

fn items() -> Vec<Item> {
    vec![
        Item {
            kind: Kind::PullRequest,
            number: 48,
            title: "Centre the worktree dialog".into(),
            url: "https://github.com/penso/herdr-gpui/pull/48".into(),
            author: "penso".into(),
            head: Some("worktree/rapid-forest".into()),
            fork_owner: None,
            draft: false,
        },
        Item {
            kind: Kind::PullRequest,
            number: 51,
            title: "Fork contribution".into(),
            url: "https://github.com/penso/herdr-gpui/pull/51".into(),
            author: "outsider".into(),
            head: Some("patch-1".into()),
            fork_owner: Some("outsider".into()),
            draft: true,
        },
        Item {
            kind: Kind::Issue,
            number: 1255,
            title: "bug: agent end message is empty".into(),
            url: "https://github.com/penso/herdr-gpui/issues/1255".into(),
            author: "penso".into(),
            head: None,
            fork_owner: None,
            draft: false,
        },
    ]
}

/// Open the new worktree dialog on the repository workspace, already on `tab`
/// and with a listing in hand, so the tabs never reach GitHub or the disk.
fn open_dialog(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, connected: bool, tab: Tab) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
            view.live.status = crate::state::ConnectionStatus::Connected;
            // The listing reads a checkout, so it is owned-local-socket only.
            view.live.local_daemon_peer = true;
            view.menu.reset();
            view.menu.github = if connected {
                crate::github::Auth::connected_fixture()
            } else {
                Default::default()
            };
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
            if connected {
                let source = view.menu.worktree.as_mut().unwrap();
                source.install(origin(), items());
                view.select_worktree_tab(tab, window, cx);
            }
        });
    });
    draw(cx);
}

/// GPUI double-buffers frames and their debug bounds, so a selector that was
/// painted two frames ago is still readable. Draw both buffers before asking
/// whether something is on screen.
fn draw(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }
}

fn tab(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Tab {
    cx.update(|_, cx| view.read(cx).menu.worktree.as_ref().unwrap().tab)
}

/// Neither GitHub listing exists without an account, so those tabs do not move
/// and the dialog stays the branch form it has always been.
#[gpui::test]
fn github_tabs_are_inert_until_an_account_is_connected(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    for selector in [
        "worktree-search",
        "worktree-tab-new",
        "worktree-tab-existing",
        "worktree-tab-branch",
        "worktree-tab-PR",
        "worktree-tab-issues",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_worktree_tab(Tab::Items(Kind::PullRequest), window, cx);
        });
    });
    draw(cx);
    assert_eq!(tab(&view, cx), Tab::New);
    // The branch field and its checkout preview are still what the dialog
    // shows, and no listing was requested for an account that does not exist.
    assert!(cx.debug_bounds("dialog-input").is_some());
    assert!(cx.debug_bounds("dialog-checkout").is_some());
    assert!(cx.debug_bounds("worktree-row-0").is_none());
    cx.update(|_, cx| {
        let source = view.read(cx).menu.worktree.as_ref().unwrap();
        assert!(!source.lookup.loading && source.lookup.message.is_none());
    });
}

/// With an account, a GitHub tab replaces the branch form with its listing.
#[gpui::test]
fn a_connected_account_turns_the_dialog_into_a_picker(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, true, Tab::Items(Kind::Issue));
    assert_eq!(tab(&view, cx), Tab::Items(Kind::Issue));
    assert!(cx.debug_bounds("worktree-row-0").is_some());
    assert!(cx.debug_bounds("worktree-status").is_some());
    // A picked row creates its own checkout, so this tab has no branch to type
    // and no submit button; cancelling is still the way out.
    assert!(cx.debug_bounds("dialog-input").is_none());
    assert!(cx.debug_bounds("dialog-submit").is_none());
    assert!(cx.debug_bounds("dialog-cancel").is_some());
    // Rows stay inside the panel rather than growing it past the window.
    let panel = cx.debug_bounds("menu-panel").unwrap();
    let row = cx.debug_bounds("worktree-row-0").unwrap();
    let status = cx.debug_bounds("worktree-status").unwrap();
    assert!(panel.contains(&row.origin) && row.right() <= panel.right());
    assert!(row.bottom() <= status.top());
    assert!(status.bottom() <= panel.bottom());
    assert!(panel.bottom() <= gpui::px(700.));
}

/// Each tab lists only its own kind, the search narrows it as it is typed, and
/// none of that typing reaches the branch draft behind the tab.
#[gpui::test]
fn searching_a_listing_filters_its_own_rows_only(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, true, Tab::New);
    let branch = cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().text.clone());

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_worktree_tab(Tab::Items(Kind::PullRequest), window, cx);
        });
    });
    draw(cx);
    let numbers = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let source = view.read(cx).menu.worktree.as_ref().unwrap();
            (0..source.filtered.len())
                .filter_map(|row| source.item(row).map(|item| item.number))
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(numbers(cx), vec![48, 51]);

    cx.simulate_input("fork");
    draw(cx);
    assert_eq!(numbers(cx), vec![51]);
    // Searching an author works as well as searching a title or a number.
    cx.update(|_, cx| {
        let search = view.read(cx).menu.worktree.as_ref().unwrap().search.clone();
        search.update(cx, |input, cx| input.clear(cx));
    });
    cx.simulate_input("penso");
    draw(cx);
    assert_eq!(numbers(cx), vec![48]);

    // The issue tab shares the one listing but shows only issues.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_worktree_tab(Tab::Items(Kind::Issue), window, cx);
        });
    });
    draw(cx);
    assert_eq!(numbers(cx), vec![1255]);

    // Everything typed went to the search field, not the branch behind it.
    assert_eq!(
        cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().text.clone()),
        branch
    );
}

/// What a picked row asks the daemon for: a pull request checks its own head
/// branch out, an issue gets a new branch named after it, and both name the
/// checkout for what it is for.
#[gpui::test]
fn a_picked_row_names_the_branch_base_and_label_it_creates(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    open_dialog(&view, cx, true, Tab::Items(Kind::PullRequest));
    cx.update(|_, cx| {
        let view = view.read(cx);
        let target = view.menu.target.as_ref().unwrap();
        let snapshot = view.live.snapshot.as_ref().unwrap();
        let items = items();

        let (method, params) = item_request(target, snapshot, &items[0]).unwrap();
        assert_eq!(method, herdr_client::Method::WorktreeCreate);
        assert_eq!(params["branch"], "worktree/rapid-forest");
        // An existing head may live only on the remote, so the base names the
        // tracking ref rather than the source workspace's HEAD.
        assert_eq!(params["base"], "refs/remotes/origin/worktree/rapid-forest");
        assert_eq!(params["label"], "#48 Centre the worktree dialog");
        assert_eq!(params["trust_repository"], false);

        let (_, params) = item_request(target, snapshot, &items[1]).unwrap();
        assert_eq!(params["branch"], "pr/51");
        assert_eq!(params["base"], "refs/herdr/pull/51/head");
        assert_eq!(params["trust_repository"], false);

        let (_, params) = item_request(target, snapshot, &items[2]).unwrap();
        assert_eq!(params["branch"], "1255-bug-agent-end-message-is-empty");
        // An issue's branch is new, so it starts from the dialog's own base.
        assert_eq!(params["base"], "HEAD");
        assert_eq!(params["label"], "#1255 bug: agent end message is empty");
    });
}

/// Picking a row is the whole action, and it is the only one running: a fork
/// pull request holds the
/// dialog while its branch is fetched, and a refusal releases the row again.
#[gpui::test]
fn picking_a_row_creates_its_checkout(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, true, Tab::Items(Kind::PullRequest));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.create_from_repo_item(1, cx);
            assert!(view.menu.creation.is_none());

            view.create_from_repo_item(0, cx);
            let source = view.menu.worktree.as_ref().unwrap();
            // The dialog holds the row it is creating, so a second pick cannot
            // start a second checkout while the first is still running.
            assert!(source.busy());
            assert!(matches!(
                &source.pending,
                Some(Pending::Item(item)) if item.number == 51
            ));
            assert!(source.lookup.loading);
            assert!(view.menu.error.is_none());
            assert_eq!(
                view.menu.page,
                Some(Page::Dialog(WorkspaceAction::NewWorktree))
            );
        });
    });

    // A refusal releases the row so the listing can be used again.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.creation = Some("create".into());
            view.apply_creation_response(
                Ok(serde_json::json!({"error":{"code":"worktree_create_failed","message":"branch already checked out"}})),
                window,
                cx,
            );
            let source = view.menu.worktree.as_ref().unwrap();
            assert!(!source.busy());
            assert_eq!(source.tab, Tab::Items(Kind::PullRequest));
            assert!(
                view.menu
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("branch already checked out")
            );
            assert_eq!(
                view.menu.page,
                Some(Page::Dialog(WorkspaceAction::NewWorktree))
            );
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("dialog-error").is_some());
    assert!(cx.debug_bounds("worktree-row-0").is_some());
    assert!(cx.debug_bounds("dialog-input").is_none());
}

#[gpui::test]
fn action_errors_stay_visible_in_every_listing_tab(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(600.), gpui::px(500.)));
    open_dialog(&view, cx, true, Tab::Items(Kind::PullRequest));
    for selected in [
        Tab::Existing,
        Tab::Branches,
        Tab::Items(Kind::PullRequest),
        Tab::Items(Kind::Issue),
    ] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let source = view.menu.worktree.as_mut().unwrap();
                source.tab = selected;
                source.refresh();
                view.menu.error = Some("The checkout could not be created. ".repeat(100));
                cx.notify();
            })
        });
        draw(cx);
        assert_eq!(tab(&view, cx), selected);
        assert!(cx.debug_bounds("dialog-error").is_some());
        let status = cx.debug_bounds("worktree-status").unwrap();
        let footer = cx.debug_bounds("dialog-footer").unwrap();
        assert!(status.bottom() <= footer.top());
        assert!(cx.debug_bounds("dialog-input").is_none());
    }
}

/// Tab moves between the dialog's tabs, and the listing owns the keys that
/// drive its rows rather than leaking them to the branch form.
#[gpui::test]
fn keys_move_between_tabs_and_through_the_rows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, true, Tab::New);
    assert_eq!(tab(&view, cx), Tab::New);
    cx.simulate_keystrokes("tab");
    assert_eq!(tab(&view, cx), Tab::Existing);
    cx.simulate_keystrokes("tab");
    assert_eq!(tab(&view, cx), Tab::Branches);
    cx.simulate_keystrokes("tab");
    assert_eq!(tab(&view, cx), Tab::Items(Kind::PullRequest));
    cx.simulate_keystrokes("tab");
    assert_eq!(tab(&view, cx), Tab::Items(Kind::Issue));
    // The strip wraps, and shift-tab walks it the other way.
    cx.simulate_keystrokes("tab");
    assert_eq!(tab(&view, cx), Tab::New);
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(tab(&view, cx), Tab::Items(Kind::Issue));

    cx.simulate_keystrokes("shift-tab");
    draw(cx);
    let selected = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| view.read(cx).menu.worktree.as_ref().unwrap().selected)
    };
    assert_eq!(selected(cx), 0);
    cx.simulate_keystrokes("down");
    assert_eq!(selected(cx), 1);
    // The rows wrap, as the other pickers' do.
    cx.simulate_keystrokes("down");
    assert_eq!(selected(cx), 0);
    cx.simulate_keystrokes("up");
    assert_eq!(selected(cx), 1);

    // Escape still closes the dialog from a listing.
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(view.read(cx).menu.page.is_none()));
}

/// The daemon's worktree list, with the checkout Herdr already has open, one
/// it has not, and a detached one.
fn checkouts() -> serde_json::Value {
    serde_json::json!({"result": {"type": "worktree_list", "source": {
        "repo_key": "/fixture/agent-launcher/.git", "repo_name": "agent-launcher",
        "source_workspace_id": "w3"
    }, "worktrees": [
        {"path": "/fixture/agent-launcher", "branch": "main", "label": "agent-launcher",
         "is_bare": false, "is_prunable": false, "is_detached": false, "open_workspace_id": "w3"},
        {"path": "/worktrees/agent-launcher/fix-login", "branch": "fix/login", "label": "fix-login",
         "is_bare": false, "is_prunable": false, "is_detached": false},
        {"path": "/worktrees/agent-launcher/bisect", "label": "bisect",
         "is_bare": false, "is_prunable": false, "is_detached": true}
    ]}})
}

fn branches() -> Vec<Branch> {
    vec![
        Branch {
            name: "feature/login".into(),
        },
        Branch {
            name: "fix/crash".into(),
        },
    ]
}

fn install_local_listings(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let source = view.menu.worktree.as_mut().unwrap();
            source.install_checkouts(checkouts());
            source.install_branches(branches());
        });
    });
    draw(cx);
}

/// The existing tab offers only the checkouts no workspace has open, and a
/// picked one is opened by the daemon rather than created.
#[gpui::test]
fn existing_checkouts_not_open_in_herdr_are_offered_for_opening(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    install_local_listings(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_worktree_tab(Tab::Existing, window, cx);
        });
    });
    draw(cx);
    assert_eq!(tab(&view, cx), Tab::Existing);
    assert!(cx.debug_bounds("worktree-row-1").is_some());
    assert!(cx.debug_bounds("worktree-row-2").is_none());
    cx.update(|_, cx| {
        let view = view.read(cx);
        let source = view.menu.worktree.as_ref().unwrap();
        let paths: Vec<_> = source
            .checkouts
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        assert_eq!(
            paths,
            [
                "/worktrees/agent-launcher/fix-login",
                "/worktrees/agent-launcher/bisect"
            ]
        );
        let target = view.menu.target.as_ref().unwrap();
        let snapshot = view.live.snapshot.as_ref().unwrap();
        let (method, params) = pick_request(
            target,
            snapshot,
            &Pending::Checkout("/worktrees/agent-launcher/fix-login".into()),
        )
        .unwrap();
        assert_eq!(method, herdr_client::Method::WorktreeOpen);
        assert_eq!(params["path"], "/worktrees/agent-launcher/fix-login");
        assert_eq!(params["trust_repository"], false);
    });

    // An opened checkout answers as opened, and the dialog follows it.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.worktree.as_mut().unwrap().pending = Some(Pending::Checkout(
                "/worktrees/agent-launcher/fix-login".into(),
            ));
            view.menu.creation = Some("open".into());
            view.apply_creation_response(
                Ok(serde_json::json!({"result": {"type": "worktree_opened",
                    "workspace": {"workspace_id": "w9"}}})),
                window,
                cx,
            );
            assert!(view.menu.error.is_none(), "{:?}", view.menu.error);
            assert!(view.menu.page.is_none());
        });
    });
}

/// A local branch is checked out as it is, and its row is only its name, one
/// line tall where a checkout also shows its path.
#[gpui::test]
fn local_branches_without_a_checkout_are_one_line_rows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    install_local_listings(&view, cx);
    let row_height = |tab: Tab, cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.select_worktree_tab(tab, window, cx));
        });
        draw(cx);
        cx.debug_bounds("worktree-row-0").unwrap().size.height
    };
    let checkout = row_height(Tab::Existing, cx);
    let branch = row_height(Tab::Branches, cx);
    let line = cx.update(|_, cx| gpui::px(view.read(cx).config.ui.line_height()));
    // Layout rounds to device pixels, so allow a pixel either way.
    assert!(
        (checkout - branch - line).abs() <= gpui::px(1.),
        "{checkout:?} {branch:?} {line:?}"
    );
    assert!(cx.debug_bounds("worktree-row-1").is_some());
    cx.update(|_, cx| {
        let view = view.read(cx);
        let target = view.menu.target.as_ref().unwrap();
        let snapshot = view.live.snapshot.as_ref().unwrap();
        let (method, params) =
            pick_request(target, snapshot, &Pending::Branch(branches()[0].clone())).unwrap();
        assert_eq!(method, herdr_client::Method::WorktreeCreate);
        assert_eq!(params["branch"], "feature/login");
        assert_eq!(params["base"], "HEAD");
    });
}

/// One search field sits above the tabs and searches every listing: each tab
/// counts what it kept, typing from the branch form moves to the first tab
/// that matched, and the branch draft is never typed into.
#[gpui::test]
fn one_search_field_searches_every_tab(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, true, Tab::New);
    install_local_listings(&view, cx);
    let branch = cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().text.clone());
    let search = view.read_with(cx, |view, _| {
        view.menu.worktree.as_ref().unwrap().search.clone()
    });
    let tabs = cx.debug_bounds("worktree-tabs").unwrap();
    let search_bounds = cx.debug_bounds("worktree-search").unwrap();
    assert!(
        search_bounds.bottom() <= tabs.top() + gpui::px(1.),
        "{search_bounds:?} {tabs:?}"
    );
    for count in [
        "worktree-tab-count-existing",
        "worktree-tab-count-branch",
        "worktree-tab-count-PR",
        "worktree-tab-count-issues",
    ] {
        assert!(cx.debug_bounds(count).is_some(), "{count}");
    }
    assert!(cx.debug_bounds("worktree-tab-count-new").is_none());

    cx.update(|window, cx| window.focus(&search.read(cx).focus.clone(), cx));
    cx.simulate_input("login");
    draw(cx);
    // The existing tab is the first that matched, so the dialog moved there.
    assert_eq!(tab(&view, cx), Tab::Existing);
    let hits = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let source = view.read(cx).menu.worktree.as_ref().unwrap();
            Tab::ALL.map(|tab| source.hits(tab))
        })
    };
    // "fix/login" is a checkout and "feature/login" a branch; no GitHub row says so.
    assert_eq!(hits(cx), [None, Some(1), Some(1), Some(0), Some(0)]);

    cx.simulate_keystrokes("tab");
    assert_eq!(tab(&view, cx), Tab::Branches);
    cx.update(|_, cx| {
        let source = view.read(cx).menu.worktree.as_ref().unwrap();
        assert_eq!(source.filtered.len(), 1);
        assert_eq!(search.read(cx).text(), "login");
    });

    cx.update(|_, cx| search.update(cx, |input, cx| input.clear(cx)));
    cx.simulate_input("fork");
    draw(cx);
    assert_eq!(hits(cx), [None, Some(0), Some(0), Some(1), Some(0)]);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().text.clone()),
        branch
    );
}

/// Moving between tabs never resizes the dialog: the branch form takes the
/// same settled size as the listings, whatever each one holds.
#[gpui::test]
fn every_tab_keeps_the_same_dialog_size(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, true, Tab::New);
    install_local_listings(&view, cx);
    let form = cx.debug_bounds("menu-panel").unwrap();
    for tab in Tab::ALL {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.select_worktree_tab(tab, window, cx));
        });
        draw(cx);
        assert_eq!(self::tab(&view, cx), tab);
        let panel = cx.debug_bounds("menu-panel").unwrap();
        assert_eq!(panel.size, form.size, "{tab:?}");
    }
    // The form's footer still sits at the panel's bottom edge.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_worktree_tab(Tab::New, window, cx)
        });
    });
    draw(cx);
    let footer = cx.debug_bounds("dialog-footer").unwrap();
    assert!((form.bottom() - footer.bottom()).abs() <= gpui::px(2.));
}

/// The new worktree shortcut opens the same dialog as the focused workspace's
/// menu row; a linked checkout defers to its repository's main checkout, and a
/// workspace outside Git opens nothing.
#[gpui::test]
fn shortcut_opens_the_dialog_for_the_focused_repository(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    cx.update(|_, cx| crate::bind_keys(cx));
    for (focused, expected) in [("w3", Some("w3")), ("w4", Some("w3")), ("w1", Some("w1"))] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
                snapshot.focused_workspace_id = Some(focused.into());
                view.live.status = crate::state::ConnectionStatus::Connected;
                view.live.local_daemon_peer = true;
                view.menu.reset();
                view.menu.github = Default::default();
                window.focus(&view.focus, cx);
            });
        });
        draw(cx);
        cx.simulate_keystrokes("cmd-n");
        cx.update(|_, cx| {
            let menu = &view.read(cx).menu;
            assert_eq!(
                menu.page,
                expected.map(|_| Page::Dialog(WorkspaceAction::NewWorktree)),
                "{focused}"
            );
            assert_eq!(
                menu.target.as_ref().map(|target| target.id.as_str()),
                expected,
                "{focused}"
            );
        });
    }
    // Every refusal says why in the flash instead of doing nothing visible.
    use super::workspace::NewWorktreeUnavailable::*;
    type Setup = fn(&mut HerdrWindow);
    let cases: [(Setup, super::workspace::NewWorktreeUnavailable); 4] = [
        (
            |view| {
                let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                for workspace in &mut snapshot.workspaces {
                    if workspace.workspace_id == "w1" {
                        workspace.branch = None;
                    }
                }
            },
            NotGit,
        ),
        (
            |view| {
                let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                snapshot
                    .workspaces
                    .retain(|workspace| workspace.workspace_id != "w3");
                snapshot.focused_workspace_id = Some("w4".into());
            },
            MainCheckoutClosed,
        ),
        (
            |view| {
                let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                snapshot.focused_workspace_id = None;
            },
            NoWorkspace,
        ),
        (
            |view| view.live.status = crate::state::ConnectionStatus::Disconnected,
            Disconnected,
        ),
    ];
    for (setup, reason) in cases {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
                snapshot.focused_workspace_id = Some("w1".into());
                view.live.status = crate::state::ConnectionStatus::Connected;
                view.menu.reset();
                view.flash = None;
                setup(view);
                window.focus(&view.focus, cx);
            });
        });
        draw(cx);
        cx.simulate_keystrokes("cmd-n");
        draw(cx);
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.menu.page.is_none(), "{reason:?}");
            assert_eq!(
                view.flash.as_ref().map(|(flash, _)| flash),
                Some(&crate::window::Flash::warning(reason.message()))
            );
        });
        assert!(cx.debug_bounds("flash").is_some(), "{reason:?}");
    }
}

/// The name field is what the dialog opens on, and its typing never edits the
/// branch draft; the branch field is still one click away.
#[gpui::test]
fn the_name_field_opens_focused_and_keeps_its_keys(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    let branch = cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().text.clone());
    let name = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let source = view.read(cx).menu.worktree.as_ref().unwrap();
            source.name.read(cx).text().to_owned()
        })
    };
    assert!(cx.update(|window, cx| view.read(cx).worktree_name_focused(window, cx)));
    assert!(cx.debug_bounds("worktree-name").is_some());
    // Empty means the daemon's default label.
    assert_eq!(cx.update(|_, cx| view.read(cx).worktree_name(cx)), None);

    cx.simulate_input("Login work");
    cx.simulate_keystrokes("backspace left right");
    assert_eq!(name(cx), "Login wor");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).worktree_name(cx)),
        Some("Login wor".into())
    );
    cx.update(|_, cx| {
        let input = view.read(cx).menu.input.as_ref().unwrap();
        assert_eq!(input.text, branch);
        assert_eq!(input.selection, 0..branch.len());
    });

    // Clicking the branch field moves typing there, leaving the name alone.
    let field = cx.debug_bounds("dialog-input").unwrap();
    cx.simulate_click(field.center(), gpui::Modifiers::none());
    assert!(!cx.update(|window, cx| view.read(cx).worktree_name_focused(window, cx)));
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("feature/login");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().text.clone()),
        "feature/login"
    );
    assert_eq!(name(cx), "Login wor");

    // Returning to the form tab puts typing back on the name.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_worktree_tab(Tab::New, window, cx)
        });
    });
    assert!(cx.update(|window, cx| view.read(cx).worktree_name_focused(window, cx)));

    // Enter from the name field submits the form; the fixture has no daemon.
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.menu.page,
            Some(Page::Dialog(WorkspaceAction::NewWorktree))
        );
        assert!(view.menu.error.is_some());
    });
    // Escape still dismisses from the name field.
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(view.read(cx).menu.page.is_none()));
}

/// A typed name rides along on `worktree.create` as its label, so the daemon
/// names the workspace as it creates it; an empty one leaves the label out.
#[gpui::test]
fn a_typed_name_labels_the_created_worktree(cx: &mut gpui::TestAppContext) {
    use gpui::AppContext;
    use herdr_client::{
        ConnectOptions, ConnectTarget, Method, Stream, connect_with_connector,
        protocol::{endpoint::*, *},
    };
    use serde_json::{Value, json};
    use std::time::Duration;
    let (stream, mut server) = Stream::pair().unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let client = connect_with_connector(
        ConnectTarget::Socket("/unused".into()),
        ConnectOptions::default(),
        true,
        move |_, _| Ok(stream),
    )
    .unwrap();
    assert!(matches!(
        read_message(&mut server, MAX_FRAME_SIZE).unwrap(),
        ClientMessage::EndpointControl { .. }
    ));
    let mut welcome: Value = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-welcome-v1.json"
    ))
    .unwrap();
    welcome["methods"] = json!(["worktree.list", "worktree.create", "workspace.focus"]);
    let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    for (kind, data) in [
        (ENDPOINT_WELCOME_KIND, welcome.to_string()),
        (
            ENDPOINT_SNAPSHOT_KIND,
            serde_json::to_string(&snapshot).unwrap(),
        ),
    ] {
        write_message(
            &mut server,
            &ServerMessage::EndpointControl {
                kind: kind.into(),
                data,
            },
            MAX_GRAPHICS_FRAME_SIZE,
        )
        .unwrap();
    }
    let connected = client.events.recv_timeout(Duration::from_secs(3)).unwrap();
    let snapshot_event = client.events.recv_timeout(Duration::from_secs(3)).unwrap();
    // No terminal render tree: it would enqueue unrelated resize requests.
    struct Fixture(Entity<HerdrWindow>);
    impl gpui::Render for Fixture {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            gpui::div()
        }
    }
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.live.apply(connected);
            view.live.apply(snapshot_event);
            std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap()).workspaces =
                sidebar::layout_tests::snapshot(7).workspaces;
            view.endpoints[0].connection.handle = Some(client.handle.clone());
            *view.endpoints[0].connection.inbox.lock().unwrap() = view.live.clone();
        });
    });
    for name in ["  Login work ", "   "] {
        let branch = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.menu.reset();
                view.open_workspace_menu("w3", Default::default(), window, cx);
                view.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
                let source = view.menu.worktree.as_ref().unwrap();
                source
                    .name
                    .update(cx, |input, cx| input.set_text_selected(name, cx));
                view.menu.creation = None;
                view.submit_workspace_dialog(window, cx);
                assert!(view.menu.error.is_none(), "{:?}", view.menu.error);
                view.menu.input.as_ref().unwrap().text.clone()
            })
        });
        // The dialog also asks for the repository's checkouts on opening, and
        // one request is in flight at a time, so each one is answered.
        let request = loop {
            let ClientMessage::ClientShellEndpointRequest { request, .. } =
                read_message(&mut server, MAX_FRAME_SIZE).unwrap()
            else {
                continue;
            };
            let request: Value = serde_json::from_str(&request).unwrap();
            let id = request["id"].as_str().unwrap();
            let response = json!({"id": id, "error": {"code": "fixture", "message": "unused"}});
            write_message(
                &mut server,
                &ServerMessage::ClientShellEndpointResponseChunk {
                    boot_id: snapshot.boot_id.clone(),
                    request_id: id.into(),
                    final_chunk: true,
                    data: serde_json::to_vec(&response).unwrap(),
                },
                MAX_GRAPHICS_FRAME_SIZE,
            )
            .unwrap();
            if request["method"] == Method::WorktreeCreate.as_str() {
                break request;
            }
        };
        let mut params = json!({"workspace_id": "w3", "base": "HEAD", "focus": true,
            "trust_repository": false, "branch": branch});
        if !name.trim().is_empty() {
            params["label"] = json!("Login work");
        }
        assert_eq!(request["params"], params);
    }
    client.handle.disconnect();
}
