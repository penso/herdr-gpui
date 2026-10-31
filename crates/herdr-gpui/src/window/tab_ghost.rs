//! What follows the pointer once a lifted tab is carried off its strip: a
//! card showing the tab, so the drag reads as carrying it. Past the window's
//! edges, macOS draws the card in a small window of its own that follows
//! the pointer (`tab_float`). Elsewhere a window cannot be moved, so the
//! card waits at the nearest edge of this one; either way it says the drop
//! opens a new window, and the cursor turns to the copy cursor.
// i18n: migrated

use super::HerdrWindow;
use crate::i18n::fl;
use crate::{
    browser::{GroupId, Pick},
    config::FontConfig,
    fonts::StyledFont,
};
use gpui::{prelude::*, *};

/// How far below and right of the pointer the card sits, so the cursor
/// does not cover its label.
pub(super) const OFFSET: f32 = 12.;
/// The card's widest, so a long title does not cover the window.
pub(super) const MAX_WIDTH: f32 = 280.;
/// How far from the window's edges the card keeps.
const MARGIN: f32 = 8.;

/// Where a carried tab's card is drawn.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Ghost {
    pub(super) pick: Pick,
    /// The group whose strip it was lifted from, whose colors it keeps.
    pub(super) group: GroupId,
    pub(super) at: Point<Pixels>,
    /// Whether the pointer is past the window's edges, where a drop opens
    /// the tab in a new window.
    pub(super) outside: bool,
}

/// The card drawn for a carried tab, whether in this window or its own.
#[derive(Clone)]
pub(super) struct Card {
    label: SharedString,
    background: u32,
    text: u32,
    border: u32,
    font: FontConfig,
    hint_size: f32,
    /// Whether to say a drop opens a new window.
    pub(super) outside: bool,
}

impl Card {
    pub(super) fn render(&self) -> Div {
        div()
            .debug_selector(|| "tab-ghost".into())
            .flex()
            .flex_col()
            .gap(px(2.))
            .max_w(px(MAX_WIDTH))
            .px(px(12.))
            .py(px(6.))
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(self.background))
            .border_1()
            .border_color(rgb(self.border))
            .shadow_lg()
            .text_color(rgb(self.text))
            .text_font(&self.font)
            .text_size(px(self.font.size))
            .child(div().truncate().child(self.label.clone()))
            .when(self.outside, |card| {
                card.child(
                    div()
                        .debug_selector(|| "tab-ghost-new-window".into())
                        .text_size(px(self.hint_size))
                        .opacity(0.8)
                        .child(fl!("tab-strip-release-new-window")),
                )
            })
    }
}

/// Turns the cursor to the copy cursor, which the platform shows wherever
/// the pointer is, in or out of the window.
pub(super) fn copy_cursor() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        |_, _, window, _| window.set_window_cursor_style(CursorStyle::DragCopy),
    )
    .size_0()
}

impl HerdrWindow {
    /// The card to draw for a lifted tab carried off its strip: farther from
    /// where it was pressed than the strip is tall, or out of the window.
    pub(super) fn tab_ghost(&self) -> Option<Ghost> {
        let drag = self.tab_drag.as_ref().filter(|drag| drag.floating())?;
        let off = f32::from((drag.pointer.y - drag.origin.y).abs()) > self.tab_strip_height();
        (off || drag.outside).then(|| Ghost {
            pick: drag.pick.clone(),
            group: drag.group,
            at: drag.pointer,
            outside: drag.outside,
        })
    }

    /// The card for `ghost`, labelled as its strip labels the tab.
    pub(super) fn ghost_card(&self, ghost: &Ghost, cx: &App) -> Option<Card> {
        let label = match &ghost.pick {
            Pick::Herdr(id) => {
                let snapshot = self.live.snapshot.as_ref()?;
                snapshot
                    .tabs
                    .iter()
                    .find(|tab| tab.tab_id == *id)
                    .map(|tab| {
                        SharedString::from(herdr_client::text::tab_label(snapshot, tab).to_owned())
                    })
            }
            Pick::Page(_) => self
                .browser_tab_labels(cx)
                .into_iter()
                .find_map(|(page, label)| (page == ghost.pick).then_some(label)),
        }?;
        let (background, text) = self.tab_colors(true, ghost.group);
        Some(Card {
            label,
            background,
            text,
            border: self.theme.active,
            font: self.config.tabs.clone(),
            hint_size: self.config.ui.size * 0.85,
            outside: ghost.outside,
        })
    }

    /// Whether a window of its own draws the card out of this one, as on
    /// macOS unless that window could not open. While it opens, this
    /// window draws none, so the card does not flash at its edge first.
    fn card_floats(&self) -> bool {
        #[cfg(target_os = "macos")]
        return self
            .tab_drag
            .as_ref()
            .is_some_and(|drag| drag.float != super::tab_float::Float::Failed);
        #[cfg(not(target_os = "macos"))]
        false
    }

    /// The card drawn in this window. Out of it, macOS draws the card in a
    /// window of its own instead, unless that window could not open.
    pub(super) fn render_tab_ghost(&self, cx: &App) -> Option<AnyElement> {
        let ghost = self.tab_ghost()?;
        if ghost.outside && self.card_floats() {
            return Some(copy_cursor().into_any_element());
        }
        let card = self.ghost_card(&ghost, cx)?;
        Some(
            deferred(
                anchored()
                    .position(ghost.at + point(px(OFFSET), px(OFFSET)))
                    .snap_to_window_with_margin(px(MARGIN))
                    .child(
                        card.render()
                            .when(ghost.outside, |card| card.child(copy_cursor())),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
