//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the question
//! before cloud machines are created: it draws only translated messages.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

#[test]
fn creating_cloud_machines_asks_only_in_messages_under_en_xa() {
    under_en_xa("create cloud en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(1000.), px(900.)));
        draw(cx);
        recorder.take();
        view.update_in(cx, |view, window, cx| {
            view.ask_to_create_cloud(creation(CloudProvider::ALL[0], 3), window, cx)
        });
        draw(cx);
        assert_translated("Create cloud devices", &recorder.take(), &[]);
    });
}
