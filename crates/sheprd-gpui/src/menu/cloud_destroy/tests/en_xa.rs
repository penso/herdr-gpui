//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the question
//! before saved cloud devices are destroyed: it draws only translated
//! messages, except for the devices' names.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

#[test]
fn destroying_cloud_devices_asks_only_in_messages_under_en_xa() {
    under_en_xa("destroy cloud en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(1000.), px(900.)));
        draw(cx);
        recorder.take();
        let targets: Vec<_> = CloudProvider::ALL
            .iter()
            .enumerate()
            .map(|(index, &provider)| target(provider, &format!("m{index}")))
            .collect();
        view.update_in(cx, |view, window, cx| {
            view.ask_to_destroy_cloud(&targets, window, cx)
        });
        draw(cx);
        let names: Vec<String> = targets.iter().map(|target| target.label.clone()).collect();
        assert_translated("Destroy cloud devices", &recorder.take(), &names);
    });
}
