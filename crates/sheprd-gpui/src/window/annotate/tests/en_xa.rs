//! The `en-XA` leak test for a note on terminal text: the card with its
//! field, buttons, and hints, and the flash a queued note shows, draw only
//! messages, apart from the pane's own text and the note typed.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa};

#[test]
fn the_note_card_draws_only_messages() {
    under_en_xa("terminal note en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx, shown(Some("working"), false));
        let terminal = ["$ cargo test", "failed to connect", "failed"].map(str::to_owned);
        recorder.take();

        select(&view, cx);
        write(&view, cx, "");
        draw(cx);
        assert_translated("Note card", &uncut(recorder.take()), &terminal);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let input = view.terminal_notes.composer.as_ref().unwrap().input.clone();
                input.update(cx, |input, cx| input.set_text_selected("First.", cx));
            });
            window.refresh();
        });
        cx.simulate_keystrokes("shift-enter");
        select(&view, cx);
        write(&view, cx, "");
        cx.update(|window, _| window.refresh());
        draw(cx);
        let mut known = terminal.to_vec();
        known.push("First.".to_owned());
        assert_translated("Queued note and its flash", &uncut(recorder.take()), &known);
    });
}
