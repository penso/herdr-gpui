//! A long conversation lays out and paints only the rows in view.
use super::*;
use crate::{
    orchestrator::{
        Request, Timing,
        view::{Detail, DetailTab, Look, OrchestratorView},
    },
    pr_actions::{Comment, CommentKind},
};
use gpui::{TestAppContext, px};
use std::sync::Arc;

#[gpui::test]
fn a_long_conversation_draws_only_the_rows_in_view(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        gitlab_store: Default::default(),
        timing: Timing::default(),
    };
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    // A paragraph far wider than the window, then a hundred short lines.
    let body: String = std::iter::once(format!("{}\n", "wrapped ".repeat(400)))
        .chain((0..100).map(|line| format!("- line {line}\n")))
        .collect();
    let comments = (0..30)
        .map(|index| Comment {
            author: format!("author{index}"),
            kind: CommentKind::Conversation,
            body: String::new(),
            created_at: format!("2026-09-20T12:{index:02}:00Z"),
            source: body.clone(),
            avatar: None,
            bot: false,
        })
        .collect();
    let profile = crate::github::Profile::fixture("me", Arc::new("token".into()), &[]);
    let pr = item(
        Provider::Github,
        "74",
        "Pull request",
        "2026-10-04T00:00:00Z",
    );
    let key = pr.key.canonical();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().request.token = Some(crate::github::Tokens::new(&profile, []));
            view.pr.set_comments(comments);
            view.repo_mut().snapshot.items = Arc::new(vec![pr]);
            view.detail = Some(Detail {
                key,
                tab: DetailTab::Conversation,
            });
            cx.notify();
        })
    });
    cx.run_until_parked();
    let list = cx.update(|_, cx| view.read(cx).conversation.list());
    assert!(list.item_count() > 3000, "{} rows", list.item_count());
    // A card drawn across rows has no break between its bar and its body;
    // the space between entries sits above the bar.
    let card = |cx: &mut gpui::VisualTestContext, selector| cx.debug_bounds(selector).unwrap();
    let summary = card(cx, "orchestrator-conversation-card-0");
    let bar = card(cx, "orchestrator-conversation-card-1");
    let body = card(cx, "orchestrator-conversation-card-2");
    assert_eq!(bar.bottom(), body.top());
    assert_eq!(bar.top() - summary.bottom(), px(12.));
    // A long paragraph wraps inside the window rather than running past it.
    let width = cx.update(|window, _| window.viewport_size().width);
    assert!(body.right() <= width, "{:?} past {width:?}", body.right());
    // Each row keeps the page's side padding: nothing touches either edge.
    let row = card(cx, "orchestrator-conversation-row-0");
    assert_eq!(summary.left() - row.left(), px(16.));
    assert_eq!(row.right() - summary.right(), px(16.));
    assert!(cx.debug_bounds("orchestrator-conversation-row-0").is_some());
    assert!(
        cx.debug_bounds("orchestrator-conversation-row-1500")
            .is_none()
    );
    // The list lays out once before its handle knows how far it scrolls; in
    // the app the scrollbar's refresh draws that frame.
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    };
    draw(cx);
    // The scrollbar's thumb starts at the list's top, along its right edge.
    let thumb = card(cx, "orchestrator-conversation-thumb");
    let viewport = list.viewport_bounds();
    assert_eq!(thumb.top(), viewport.top());
    assert_eq!(thumb.right(), viewport.right());
    // Rows not yet measured still count, so the thumb is sized for them all.
    assert!(thumb.size.height < viewport.size.height / 4., "{thumb:?}");
    // Scrolled there, the far row draws and the first no longer does.
    list.scroll_to(gpui::ListOffset {
        item_ix: 1500,
        offset_in_item: px(0.),
    });
    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("orchestrator-conversation-row-1500")
            .is_some()
    );
    assert!(cx.debug_bounds("orchestrator-conversation-row-0").is_none());
    // Halfway through the rows, the thumb sits about halfway down.
    draw(cx);
    let moved = card(cx, "orchestrator-conversation-thumb");
    let middle = (moved.top() - viewport.top()) / (viewport.size.height - moved.size.height);
    assert!((0.3..0.7).contains(&middle), "{moved:?} in {viewport:?}");
}
