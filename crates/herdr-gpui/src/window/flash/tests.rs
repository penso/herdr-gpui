use super::*;
use crate::sidebar::layout_tests::fixture_window;
use gpui::TestAppContext;

#[gpui::test]
fn a_flash_fades_in_takes_new_text_in_place_and_fades_out(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        assert!(view.flash_shown().is_none());
        view.show_flash(Flash::success("copied"), cx);
        let (_, shown) = view.flash_shown().expect("a flash shows");
        assert!(shown < 1., "{shown}");
        assert!(view.flash_moving());

        // A second gesture answered while one is up does not fade again.
        let since = view.flash_motion.since;
        view.show_flash(Flash::warning("nothing to copy"), cx);
        assert_eq!(view.flash_motion.since, since);

        let expires = view.flash.as_ref().map(|(_, expires)| *expires);
        assert!(view.tick_flash(expires.unwrap_or_else(Instant::now)));
        assert!(view.flash.is_none());
        let (flash, shown) = view.flash_shown().expect("it fades out");
        assert_eq!(flash.text, "nothing to copy");
        assert!(shown > 0.9, "{shown}");
        assert!(view.flash_moving());

        // A new one cancels the fading copy.
        view.show_flash(Flash::hint("try this"), cx);
        assert!(view.flash_motion.leaving.is_none());
    });
}
