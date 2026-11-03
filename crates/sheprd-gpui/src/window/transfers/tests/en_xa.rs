//! The `en-XA` leak test for the file copy card: copying, finishing, and
//! cancelling draw only messages and the copy's own file name.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

#[test]
fn the_copy_card_draws_only_messages() {
    under_en_xa("file copy en-XA leak", |cx, recorder| {
        let (fixture, cx) = cx.add_window_view(fixture);
        let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
        let peer = Peer::new();
        let transfer = view.update(cx, |view, cx| {
            peer.prepare(view, cx);
            let transfer = pending(view, InputTarget::Pane("w1:p1".into()));
            let shared = (
                transfer.sent.clone(),
                transfer.total.clone(),
                transfer.cancelled.clone(),
            );
            view.file_transfer = Some(transfer);
            shared
        });
        let (sent, total, cancelled) = transfer;
        for (surface, done, cancel) in [
            ("Copying", 512, false),
            ("Finishing", 2048, false),
            ("Cancelling", 512, true),
        ] {
            sent.store(done, Ordering::Relaxed);
            total.store(2048, Ordering::Relaxed);
            cancelled.store(cancel, Ordering::Relaxed);
            view.update(cx, |view, cx| view.poll_file_transfer(cx));
            cx.update(|window, _| window.refresh());
            draw(cx);
            assert_translated(surface, &recorder.take(), &["large file".to_owned()]);
        }
    });
}
