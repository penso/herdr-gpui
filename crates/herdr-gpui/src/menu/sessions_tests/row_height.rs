//! A row grows with a note that wraps instead of letting it spill into the
//! row below.
use super::*;

/// `en-XA` lengthens the local Add row's note past the popup's width.
#[test]
fn a_wrapped_note_makes_its_row_taller() {
    crate::i18n::leak_tests::under_en_xa("sessions row height", |cx, _| {
        let (_view, cx) = on_the_first_device(cx, None);
        let short = bounds(cx, "sessions-detail-0");
        let wrapped = bounds(cx, "sessions-detail-1");
        assert!(
            wrapped.size.height > short.size.height,
            "the fixture's note must wrap: {wrapped:?}"
        );
        let row = bounds(cx, "sessions-row-1");
        assert!(
            wrapped.bottom() + px(8.) <= row.bottom(),
            "{wrapped:?} spills out of {row:?}"
        );
        assert!(row.size.height > bounds(cx, "sessions-row-0").size.height);
    });
}
