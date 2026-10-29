//! The `en-XA` leak test for surfaces of the main window
//! (`crate::i18n::leak_tests`). Other windows keep theirs in their own
//! module's `tests/en_xa.rs`.

use crate::{
    HerdrWindow,
    i18n::leak_tests::{Recorder, assert_translated, draw, under_en_xa},
    palette::Filter,
};
use gpui::TestAppContext;

/// A surface the test opens, and the text in it that is data, not the
/// app's: names users chose, and what the daemon or the keymap reports.
struct Surface {
    name: &'static str,
    open: fn(&mut HerdrWindow, &mut gpui::Window, &mut gpui::Context<HerdrWindow>),
    data: fn(&HerdrWindow) -> Vec<String>,
}

const SURFACES: &[Surface] = &[Surface {
    name: "command palette, Commands",
    open: |view, window, cx| view.open_palette(Filter::Commands, window, cx),
    // Each row's shortcut.
    data: |view| {
        crate::controls::COMMANDS
            .iter()
            .map(|info| view.keymap().primary(info.command).to_owned())
            .collect()
    },
}];

#[test]
fn every_migrated_surface_shows_only_translated_text_under_en_xa() {
    under_en_xa("en-XA leak", |cx, recorder| {
        for surface in SURFACES {
            check(surface, recorder, cx);
        }
    });
}

fn check(surface: &Surface, recorder: &Recorder, cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::palette::fixture_window);
    draw(cx);
    recorder.take();
    cx.update(|window, cx| view.update(cx, |view, cx| (surface.open)(view, window, cx)));
    draw(cx);
    let data = view.read_with(cx, |view, _| (surface.data)(view));
    assert_translated(surface.name, &recorder.take(), &data);
}
