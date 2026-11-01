//! The `en-XA` leak test for the find bar and copy mode: the bar's field,
//! its count, a failed search, and copy mode's corner label draw only messages, apart from
//! the query typed.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa};

#[test]
fn the_find_bar_and_copy_mode_draw_only_messages() {
    under_en_xa("find en-XA leak", |cx, recorder| {
        let (view, mut peer, cx) = open(cx, &["pane.copy_search", "pane.scroll"]);
        view.update(cx, |view, _| view.live.supports_copy_motion = true);
        recorder.take();

        find(&view, cx);
        assert_translated("Empty find bar", &uncut(recorder.take()), &[]);

        cx.simulate_keystrokes("x");
        let request = next_request(&mut peer);
        answer(
            &view,
            &mut peer,
            cx,
            &request,
            matches(&[(110, 0, 1)], 0, 0, 3),
        );
        cx.update(|window, _| window.refresh());
        draw(cx);
        assert_translated("Find count", &uncut(recorder.take()), &["x".to_owned()]);

        answer_none(&view, &mut peer, cx);
        assert_translated("No results", &uncut(recorder.take()), &["xy".to_owned()]);

        // A search the client could not queue says why, in the shown language.
        view.update(cx, |view, cx| {
            if let Some(bar) = view.find.as_mut() {
                bar.search.send_failed(&herdr_client::Error::Full);
            }
            cx.notify();
        });
        draw(cx);
        assert_translated("Failed search", &uncut(recorder.take()), &["xy".to_owned()]);

        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| view.update(cx, |view, cx| view.enter_copy_mode(window, cx)));
        cx.update(|window, _| window.refresh());
        draw(cx);
        assert_translated("Copy mode", &uncut(recorder.take()), &[]);
    });
}

/// Types another letter and answers that it matches nothing.
fn answer_none(view: &Entity<HerdrWindow>, peer: &mut MockPeer, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("y");
    let request = next_request(peer);
    answer(view, peer, cx, &request, matches(&[], 0, 0, 0));
    cx.update(|window, _| window.refresh());
    draw(cx);
}
