use super::*;
use crate::sidebar::layout_tests::run_frames;
use gpui::{IntoElement, ParentElement, Render, TestAppContext, div};
use std::{cell::Cell, rc::Rc, time::Duration};

/// Every element that moves, by the selector its mark carries.
const MOVING: [&str; 7] = [
    "status-orbit",
    "status-spin",
    "status-wave",
    "status-chase",
    "status-braille",
    "status-braille-fill",
    "status-orca-ring",
];

struct Slot {
    status: AgentStatus,
    indicators: Indicators,
}

impl Render for Slot {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        let font = FontConfig {
            family: "Menlo".into(),
            size: 12.,
            fallbacks: None,
            line_height_multiple: None,
            weight: gpui::FontWeight::NORMAL,
        };
        div().child(status_indicator(self.status, &font, self.indicators))
    }
}

/// The moving mark `icons` draws for `status`, if any. Working always moves;
/// only the sets after Superset also move when an agent waits on you.
fn expected(icons: StatusIcons, status: AgentStatus) -> Option<&'static str> {
    match (status, icons) {
        (AgentStatus::Working, StatusIcons::Dots) => Some("status-orbit"),
        (AgentStatus::Working, StatusIcons::Solid | StatusIcons::Outline | StatusIcons::Tiles) => {
            Some("status-spin")
        }
        (AgentStatus::Working, StatusIcons::Wave) => Some("status-wave"),
        (AgentStatus::Working, StatusIcons::Braille) => Some("status-braille"),
        (AgentStatus::Working, StatusIcons::Orca) => Some("status-orca-ring"),
        (AgentStatus::Blocked, StatusIcons::Wave) => Some("status-chase"),
        (AgentStatus::Blocked, StatusIcons::Braille) => Some("status-braille-fill"),
        _ => None,
    }
}

/// Every set's working mark moves, and the settled statuses stay still but
/// for a blocked agent in the Superset-style sets. Each mark stays in the
/// slot the set reserves, so rows keep their geometry.
#[gpui::test]
fn every_set_moves_while_working_and_rests_when_settled(cx: &mut TestAppContext) {
    for &icons in StatusIcons::ALL {
        for status in STATUSES {
            let indicators = Indicators::new(None, icons, false, false, &Theme::default());
            let (_, cx) = cx.add_window_view(|_, _| Slot { status, indicators });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let want = expected(icons, status);
            for selector in MOVING {
                let bounds = cx.debug_bounds(selector);
                assert_eq!(
                    bounds.is_some(),
                    want == Some(selector),
                    "{icons:?} {status:?} {selector}"
                );
                if let Some(bounds) = bounds {
                    let font = FontConfig {
                        family: "Menlo".into(),
                        size: 12.,
                        fallbacks: None,
                        line_height_multiple: None,
                        weight: gpui::FontWeight::NORMAL,
                    };
                    let slot = gpui::px(indicators.width(&font));
                    assert!(
                        bounds.size.width <= slot && bounds.size.height <= slot,
                        "{icons:?} {status:?} {bounds:?}"
                    );
                }
            }
        }
    }
}

/// A moving mark keeps asking for its next frame as the clock runs, and a
/// settled one never does. Under reduced motion nothing asks: GPUI holds
/// every animation on its first frame.
#[gpui::test]
fn moving_marks_redraw_as_time_passes_and_rest_under_reduced_motion(cx: &mut TestAppContext) {
    for reduced in [false, true] {
        cx.update(|cx| cx.set_reduce_motion(reduced));
        for &icons in StatusIcons::ALL {
            for status in STATUSES {
                let indicators = Indicators::new(None, icons, false, false, &Theme::default());
                let (view, cx) = cx.add_window_view(|_, _| Slot { status, indicators });
                let frames = Rc::new(Cell::new(0));
                let counted = frames.clone();
                cx.update(|_, cx| {
                    cx.observe(&view, move |_, _| counted.set(counted.get() + 1))
                        .detach()
                });
                cx.update(|window, cx| window.draw(cx).clear(cx));
                // Long enough for the slowest mark's next frame, each time.
                run_frames(cx, 4, Duration::from_millis(150));
                let moves = expected(icons, status).is_some() && !reduced;
                let asked = frames.get();
                if moves {
                    assert!(asked >= 4, "{icons:?} {status:?} asked for {asked} frames");
                } else {
                    assert_eq!(asked, 0, "{icons:?} {status:?} reduced={reduced}");
                }
            }
        }
    }
}
