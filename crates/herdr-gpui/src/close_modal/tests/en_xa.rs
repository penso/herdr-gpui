//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the close
//! confirmation, of a pane and of a tab: each draws only translated
//! messages, except for the pane's or tab's name.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

#[test]
fn closing_a_pane_or_a_tab_asks_only_in_messages_under_en_xa() {
    under_en_xa("close confirmation en-XA leak", |cx, recorder| {
        let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
            view.live.snapshot = Some(Arc::new(snapshot.clone()));
            view
        });
        cx.simulate_resize(size(px(1000.), px(800.)));
        draw(cx);
        recorder.take();
        for command in [Command::ClosePane, Command::CloseTab] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.dismiss_menu(window, cx);
                    view.open_close_confirmation(command, window, cx);
                })
            });
            draw(cx);
            let label = view.read_with(cx, |view, _| {
                view.menu.close.as_ref().unwrap().label.clone()
            });
            assert_translated(&format!("{command:?}"), &recorder.take(), &[label]);
        }
    });
}
