//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the worktree
//! scripts trust page.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa};
use std::sync::Arc;

#[test]
fn the_trust_page_shows_only_messages_under_en_xa() {
    under_en_xa("worktree scripts en-XA leak", |cx, recorder| {
        let peer = MockPeer::advertising(&["worktree.list", "tab.create"]);
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|_, cx| view.update(cx, |view, cx| connect(view, &peer, cx)));
        draw(cx);
        // What the connected window already shows is not the page's.
        recorder.take();
        let config =
            config::Config::parse(b"[scripts]\nrun = \"make dev\"\nsetup = \"npm ci\"\n").unwrap();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.worktree_script = Some(super::super::Job {
                    launch: launch(view, ScriptKind::Run, None),
                    step: Step::Asking {
                        config,
                        shown: None,
                    },
                    cancel: Arc::default(),
                });
                view.poll_worktree_script(window, cx);
                assert_eq!(view.menu.page, Some(Page::WorktreeScript));
            })
        });
        draw(cx);
        // The scripts themselves and the other script's key are the file's;
        // the workspace and agent names the sidebar redraws are the daemon's.
        let data = [
            "make dev", "npm ci", "setup", "focus", "repo", "shell", "Claude",
        ]
        .map(str::to_owned);
        assert_translated("trust page", &uncut(recorder.take()), &data);
    });
}
