//! The `en-XA` leak test for a discarded image or paste: the status line
//! and the notice's title and body are whole messages, with the error's own
//! message inside.
use super::*;
use crate::i18n::leak_tests::under_en_xa;
use herdr_i18n::pseudo::is_marked;

#[test]
fn a_discarded_preparation_says_so_in_messages() {
    under_en_xa("image preparation en-XA leak", |cx, _| {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        for kind in [PreparationKind::Image, PreparationKind::Input] {
            view.update(cx, |view, cx| {
                view.config.notifications.enabled = false;
                view.endpoints[0].toasts.entries.clear();
                view.report_preparation_error(kind, &Error::ImageDecodeLimit, cx);
                let (_, notice) = view.endpoints[0].toasts.entries.back().unwrap();
                let body = notice.body.as_deref().unwrap();
                let status = view.local_error.as_deref().unwrap();
                for text in [notice.title.as_str(), body, status] {
                    assert!(is_marked(text), "not a message: {text}");
                }
                // The error inside is a message too, not its English Display.
                assert!(!body.contains(&Error::ImageDecodeLimit.to_string()));
            });
        }
    });
}
