//! Rows keep their author beside an open item while the title has room.
use super::*;
use crate::orchestrator::{
    PullRequest,
    view::{RepoSpec, repo::Repo, rows::Tab},
};
use gpui::{VisualTestContext, px, size};
use std::sync::Arc;

use crate::orchestrator::view::list::AUTHOR_TITLE_MIN;

const AUTHOR: &str = "orchestrator-row-author";
const TITLE: &str = "orchestrator-row-title";
/// The window the cutoff tests resize the pane within.
const WINDOW: f32 = 1400.;

fn issue() -> Item {
    item(Provider::Github, "377", "Icons", "2026-10-04T00:00:00Z")
}

fn pull_request() -> Item {
    pull_request_from(Provider::Github)
}

fn pull_request_from(provider: Provider) -> Item {
    let mut pr = item(provider, "pr/8", "Menus", "2026-10-04T00:00:00Z");
    pr.pull_request = Some(PullRequest {
        number: 8,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: "feature".into(),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    pr
}

/// Lists `listed` on `tab` with it open beside the list, in a window
/// `width` wide, and answers whether its row names the author.
fn author_beside_open_item(
    cx: &mut gpui::TestAppContext,
    tab: Tab,
    listed: Item,
    width: f32,
) -> bool {
    let (_dir, view, cx) = empty::open(cx);
    cx.simulate_resize(size(px(width), px(600.)));
    let key = listed.key.canonical();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.items = Arc::new(vec![listed]);
            view.tab = tab;
            view.refresh_rows();
            view.open_detail(key, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-pane").is_some());
    shown(cx)
}

fn shown(cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds(AUTHOR).is_some()
}

#[gpui::test]
fn a_wide_list_keeps_the_issue_author_beside_an_open_item(cx: &mut gpui::TestAppContext) {
    assert!(author_beside_open_item(cx, Tab::Issues, issue(), 1400.));
}

#[gpui::test]
fn a_narrow_list_drops_the_issue_author_for_the_title(cx: &mut gpui::TestAppContext) {
    assert!(!author_beside_open_item(cx, Tab::Issues, issue(), 900.));
}

#[gpui::test]
fn a_wide_list_keeps_the_pull_request_author_beside_an_open_item(cx: &mut gpui::TestAppContext) {
    assert!(author_beside_open_item(
        cx,
        Tab::PullRequests,
        pull_request(),
        1400.
    ));
}

#[gpui::test]
fn a_narrow_list_drops_the_pull_request_author_for_the_title(cx: &mut gpui::TestAppContext) {
    assert!(!author_beside_open_item(
        cx,
        Tab::PullRequests,
        pull_request(),
        900.
    ));
}

#[gpui::test]
fn resizing_the_pane_moves_the_author_across_the_fit(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    cx.simulate_resize(size(px(1400.), px(600.)));
    let listed = issue();
    let key = listed.key.canonical();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.items = Arc::new(vec![listed]);
            view.tab = Tab::Issues;
            view.refresh_rows();
            view.open_detail(key, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(shown(cx));
    let set_pane = |width: f32, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.pane_width = width;
                cx.notify();
            })
        });
        cx.run_until_parked();
    };
    set_pane(900., cx);
    assert!(!shown(cx));
    set_pane(520., cx);
    assert!(shown(cx));
}

/// Opens an item on `tab`, with the source and project columns when
/// `optional`, then holds the list at the width `author_fit` names and one
/// pixel under it. The title cell is measured from the layout, so a column,
/// gap, or padding `author_fit` miscounts shows here as a title off its
/// minimum rather than agreeing with its own arithmetic.
fn check_cutoff(cx: &mut gpui::TestAppContext, tab: Tab, optional: bool) {
    let (_dir, view, cx) = empty::open(cx);
    cx.simulate_resize(size(px(WINDOW), px(600.)));
    let (listed, other) = match tab {
        Tab::Issues => (
            issue(),
            item(Provider::Beads, "hg-1", "Store", "2026-10-03T00:00:00Z"),
        ),
        Tab::PullRequests => (pull_request(), pull_request_from(Provider::Gitlab)),
    };
    let key = listed.key.canonical();
    let fit = cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut items = vec![listed];
            if optional {
                items.push(other);
                let mut request = view.repos[0].request.clone();
                request.checkout.push_str("-site");
                view.repos.push(Repo::start(RepoSpec {
                    key: "site".into(),
                    host: "local".into(),
                    request,
                }));
            }
            view.repo_mut().snapshot.items = Arc::new(items);
            view.tab = tab;
            view.refresh_rows();
            view.open_detail(key, cx);
            cx.notify();
            assert_eq!(view.mixed_sources, optional);
            assert_eq!(view.listing_projects(), optional);
            view.author_fit()
        })
    });
    let hold_list_at = |width: f32, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.pane_width = WINDOW - width;
                cx.notify();
            })
        });
        cx.run_until_parked();
        let list = cx.debug_bounds("orchestrator-list").unwrap();
        assert_eq!(f32::from(list.size.width), width);
    };
    hold_list_at(fit, cx);
    assert!(shown(cx));
    let title = cx.debug_bounds(TITLE).unwrap();
    assert_eq!(f32::from(title.size.width), AUTHOR_TITLE_MIN);
    hold_list_at(fit - 1., cx);
    assert!(!shown(cx));
}

#[gpui::test]
fn an_issue_keeps_its_title_minimum_at_the_cutoff(cx: &mut gpui::TestAppContext) {
    check_cutoff(cx, Tab::Issues, false);
}

#[gpui::test]
fn an_issue_counts_its_optional_columns_at_the_cutoff(cx: &mut gpui::TestAppContext) {
    check_cutoff(cx, Tab::Issues, true);
}

#[gpui::test]
fn a_pull_request_keeps_its_title_minimum_at_the_cutoff(cx: &mut gpui::TestAppContext) {
    check_cutoff(cx, Tab::PullRequests, false);
}

#[gpui::test]
fn a_pull_request_counts_its_optional_columns_at_the_cutoff(cx: &mut gpui::TestAppContext) {
    check_cutoff(cx, Tab::PullRequests, true);
}
