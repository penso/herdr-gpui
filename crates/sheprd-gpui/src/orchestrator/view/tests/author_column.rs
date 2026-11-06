//! Rows keep their author beside an open item while the title has room.
use super::*;
use crate::orchestrator::{PullRequest, view::rows::Tab};
use gpui::{VisualTestContext, px, size};
use std::sync::Arc;

const AUTHOR: &str = "orchestrator-row-author";

fn issue() -> Item {
    item(Provider::Github, "377", "Icons", "2026-10-04T00:00:00Z")
}

fn pull_request() -> Item {
    let mut pr = item(Provider::Github, "pr/8", "Menus", "2026-10-04T00:00:00Z");
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
