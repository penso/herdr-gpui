//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the tab menu and
//! its pages: each draws only translated messages, except for the tab's
//! label and the workspaces it can move to.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

fn snapshot() -> ClientShellSnapshot {
    serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap()
}

fn open(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, action: Option<Action>) {
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.dismiss_menu(window, cx);
            v.open_tab_menu("w1:t1", None, point(px(20.), px(20.)), window, cx);
            if let Some(action) = action {
                v.activate_tab_menu(action, window, cx);
            }
        })
    });
    draw(cx);
}

#[test]
fn the_tab_menu_and_its_pages_show_only_messages_under_en_xa() {
    under_en_xa("tab menu en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
            view.live.snapshot = Some(Arc::new(snapshot()));
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            view
        });
        cx.simulate_resize(size(px(1000.), px(800.)));
        draw(cx);
        recorder.take();
        let snapshot = snapshot();
        open(&view, cx, None);
        assert_translated("Tab menu", &recorder.take(), &[]);

        open(&view, cx, Some(Action::Rename));
        let label = snapshot.tabs[0].label.clone();
        assert_translated("Tab menu > Rename", &recorder.take(), &[label]);

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            open(&view, cx, Some(Action::MoveToWorkspace));
            let workspaces: Vec<_> = snapshot
                .workspaces
                .iter()
                .map(|workspace| workspace.label.clone())
                .collect();
            assert_translated("Tab menu > Move", &recorder.take(), &workspaces);
        }
    });
}
