#![allow(clippy::unwrap_used)]
use super::*;
use crate::config::Theme;
use core::prelude::v1::test;
use std::{cell::Cell, rc::Rc};

struct Harness {
    picked: Rc<Cell<Option<u8>>>,
    element_picked: Rc<Cell<Option<u8>>>,
    off_picked: Rc<Cell<Option<u8>>>,
    checked: bool,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::default();
        let picked = self.picked.clone();
        let element_picked = self.element_picked.clone();
        let off_picked = self.off_picked.clone();
        div()
            .size_full()
            .child(section(&theme, "Layout", Some(Source::Herdr)))
            .child(
                row(&theme, "row-a", "Indent")
                    .description("Pixels per level")
                    .tag(Source::HerdrUi)
                    .control(toggle(&theme, "toggle-a", self.checked, true)),
            )
            .child(
                row(&theme, "row-b", "Off")
                    .description("Original")
                    .disabled("Read-only here")
                    .control(toggle(&theme, "toggle-b", false, false)),
            )
            .child(segmented(
                &theme,
                "seg",
                &[(1u8, "One"), (2u8, "Two"), (3u8, "Three")],
                Some(2),
                true,
                move |value, _, _| picked.set(Some(value)),
            ))
            .child(segmented(
                &theme,
                "seg-off",
                &[(1u8, "One"), (2u8, "Two")],
                Some(1),
                false,
                move |value, _, _| off_picked.set(Some(value)),
            ))
            .child(segmented_elements(
                &theme,
                "seg-el",
                vec![
                    (1u8, div().child("A").into_any_element()),
                    (2u8, div().child("B").into_any_element()),
                ],
                Some(1),
                true,
                |_| true,
                move |value, _, _| element_picked.set(Some(value)),
            ))
    }
}

#[gpui::test]
fn rows_render_their_parts_and_segments_report_picks(cx: &mut TestAppContext) {
    let picked = Rc::new(Cell::new(None));
    let element_picked = Rc::new(Cell::new(None));
    let off_picked = Rc::new(Cell::new(None));
    let (_, cx) = cx.add_window_view(|_, _| Harness {
        off_picked: off_picked.clone(),
        picked: picked.clone(),
        element_picked: element_picked.clone(),
        checked: true,
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx.debug_bounds("row-a").unwrap();
    assert!(row.size.height >= px(ROW_HEIGHT), "{row:?}");
    assert!(cx.debug_bounds("row-a-title").is_some());
    assert!(cx.debug_bounds("row-a-description").is_some());
    assert!(cx.debug_bounds("row-a-tag").is_some());
    assert!(cx.debug_bounds("toggle-a").is_some());
    assert!(
        cx.debug_bounds("row-b-reason").is_some(),
        "disabled reason is shown"
    );
    assert!(
        cx.debug_bounds("row-b-description").is_none(),
        "disabled reason replaces description"
    );
    let track = cx.debug_bounds("toggle-a").unwrap();
    // The shared switch at 20 px high: about 1.64 times as wide.
    assert_eq!((track.size.width, track.size.height), (px(33.), px(20.)));
    // Already-selected and disabled segments never report picks.
    let selected = cx.debug_bounds("seg-1").unwrap().center();
    cx.simulate_click(selected, Default::default());
    let off = cx.debug_bounds("seg-off-1").unwrap().center();
    cx.simulate_click(off, Default::default());
    cx.run_until_parked();
    assert_eq!(picked.get(), None);
    assert_eq!(off_picked.get(), None);
    let third = cx.debug_bounds("seg-2").unwrap().center();
    cx.simulate_click(third, Default::default());
    cx.run_until_parked();
    assert_eq!(picked.get(), Some(3));
    let second = cx.debug_bounds("seg-el-1").unwrap().center();
    cx.simulate_click(second, Default::default());
    cx.run_until_parked();
    assert_eq!(element_picked.get(), Some(2));
}

/// The toggle draws the app's shared switch, so its colours keep the
/// shared contrast rules.
#[test]
fn switch_colors_follow_primary_and_contrast_in_every_theme() {
    use crate::contrast::{Contrast, luminance, ratio};
    use crate::toggles::colors as switch_colors;

    for &name in Theme::BUILTIN_NAMES {
        for contrast in [Contrast::Standard, Contrast::High] {
            let mut theme = Theme::builtin(name).unwrap().with_contrast(contrast);
            for checked in [false, true] {
                let (track, thumb) = switch_colors(&theme, checked);
                assert_eq!(
                    track,
                    if checked {
                        theme.primary()
                    } else {
                        theme.active
                    }
                );
                assert!(
                    ratio(thumb, track) >= contrast.mark_ratio(),
                    "{name} {contrast:?} checked={checked}: {thumb:06x} on {track:06x}"
                );
            }
            // Shared/custom themes can use a dark purple accent even on light chrome.
            theme.palette[5] = 0x8839ef;
            let (track, thumb) = switch_colors(&theme, true);
            assert_eq!(track, 0x8839ef);
            assert!(luminance(thumb) > luminance(track), "{name} {contrast:?}");
            assert!(ratio(thumb, track) >= contrast.mark_ratio());
        }
    }
}
