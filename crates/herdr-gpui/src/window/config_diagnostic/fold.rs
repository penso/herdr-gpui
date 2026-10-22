//! Opening a notice card whose lines are too long for one row each. A card
//! keeps one row per line, so it covers as little of the terminal as it can;
//! only a card that cut a line short offers to open, and an open card wraps
//! its lines, scrolling past a few rows.
//!
//! Whether a line was cut short depends on the room layout gave it, which
//! render cannot know: the terminal area's width follows the sidebar. So each
//! card measures its text column after layout and draws again when what it
//! found disagrees with the frame drawn, as the code view's outline does.
use crate::config::corners;
use gpui::{prelude::*, *};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// The trailing controls' size, the fold control and the dismiss button.
pub(super) const CONTROL: f32 = 24.;
/// Between the fold control and the dismiss button.
pub(super) const CONTROL_GAP: f32 = 4.;
/// An open card scrolls its lines past this, as the announcement's body does.
pub(super) const MAX_OPEN_HEIGHT: f32 = 240.;

/// The cards, by selector, whose text did not fit its row when last laid out.
#[derive(Debug, Default)]
pub(crate) struct Overflow(RefCell<Vec<&'static str>>);

impl Overflow {
    pub(super) fn contains(&self, card: &'static str) -> bool {
        self.0.borrow().contains(&card)
    }

    /// Records whether `card` overflows; returns whether that changed.
    fn set(&self, card: &'static str, overflows: bool) -> bool {
        let mut cards = self.0.borrow_mut();
        let position = cards.iter().position(|own| *own == card);
        match (position, overflows) {
            (None, true) => cards.push(card),
            (Some(index), false) => {
                cards.swap_remove(index);
            }
            _ => return false,
        }
        true
    }
}

/// What one card's measure needs after layout.
pub(super) struct Measure {
    pub card: &'static str,
    /// The muted caption, then the card's lines, each drawn on one row.
    pub label: SharedString,
    pub lines: Arc<[String]>,
    pub font: Font,
    pub size: Pixels,
    /// Whether this frame drew the fold control, which took its room from
    /// the column being measured.
    pub foldable: bool,
    pub overflow: Rc<Overflow>,
}

/// Fills the text column it is placed in and, once laid out, checks whether
/// any row's text is wider than the column would be without the fold
/// control, drawing again when the answer changes.
pub(super) fn measure(measure: Measure) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let room = f32::from(bounds.size.width)
                + if measure.foldable {
                    CONTROL + CONTROL_GAP
                } else {
                    0.
                };
            let rows = std::iter::once(measure.label.clone()).chain(
                measure
                    .lines
                    .iter()
                    .map(|line| SharedString::from(line.clone())),
            );
            let overflows = rows.into_iter().any(|text| {
                let run = TextRun {
                    len: text.len(),
                    font: measure.font.clone(),
                    color: black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                // GPUI cuts a row short exactly when its shaped width exceeds
                // the room, so the same comparison decides here.
                let width = window
                    .text_system()
                    .shape_line(text, measure.size, &[run], None)
                    .width;
                f32::from(width) > room
            });
            if measure.overflow.set(measure.card, overflows) {
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

/// The chevron that opens and closes a card, beside its dismiss button. It
/// points down to open, up to close, as the dispatch picker's does.
pub(super) fn chevron(
    card: &'static str,
    expanded: bool,
    theme: &crate::config::Theme,
) -> Stateful<Div> {
    div()
        .id("fold")
        .debug_selector(move || format!("{card}-fold"))
        .role(Role::Button)
        // The state is the node's expanded flag, so the name stays put.
        .aria_label("Full message")
        .flex_none()
        .size(px(CONTROL))
        .rounded(px(corners::CONTROL))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(theme.active)))
        .child(
            svg()
                .path(if expanded {
                    "icons/chevron-up.svg"
                } else {
                    "icons/chevron-down.svg"
                })
                .size(px(12.))
                .text_color(rgb(theme.foreground)),
        )
}
