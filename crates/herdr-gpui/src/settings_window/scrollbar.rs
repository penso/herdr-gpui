//! Scrollbars for Settings' section list and body: a thumb over the right
//! edge of a pane whose content runs past it, showing where the pane sits
//! and how much of it shows, and dragging it. The size and travel are the
//! tab strip's thumb math, turned on its side.
// i18n: migrated
use super::SettingsWindow;
use crate::browser::Thumb;
use gpui::{prelude::*, *};

/// How wide the thumb draws, and its margin from the pane's edge.
const WIDTH: f32 = 6.;
const INSET: f32 = 3.;

/// How opaque the thumb draws over the muted color, then hovered or held.
const ALPHA: u32 = 0x60;
const HOVER_ALPHA: u32 = 0x90;
const HELD_ALPHA: u32 = 0xc0;

/// The panes that scroll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pane {
    Sections,
    Body,
}

impl Pane {
    fn selector(self) -> &'static str {
        match self {
            Self::Sections => "settings-sections-thumb",
            Self::Body => "settings-body-thumb",
        }
    }
}

/// What a thumb drags: the pane it scrolls.
#[derive(Clone, Copy, Debug)]
struct PaneThumb(Pane);

/// The thumb for `handle`'s pane as it last laid out, with the pane's
/// bounds and how far it scrolls; none while everything fits.
fn thumb(handle: &ScrollHandle) -> Option<(Thumb, Bounds<Pixels>, f32)> {
    let bounds = handle.bounds();
    let max = f32::from(handle.max_offset().y);
    let thumb = Thumb::of(
        f32::from(bounds.size.height),
        max,
        -f32::from(handle.offset().y),
    )?;
    Some((thumb, bounds, max))
}

impl SettingsWindow {
    fn pane_scroll(&self, pane: Pane) -> &ScrollHandle {
        match pane {
            Pane::Sections => &self.navigation_scroll,
            Pane::Body => &self.body_scroll,
        }
    }

    fn grab_thumb(&mut self, pane: Pane, y: Pixels) {
        if let Some((thumb, bounds, _)) = thumb(self.pane_scroll(pane)) {
            self.thumb_grab = f32::from(y - bounds.top()) - thumb.left;
        }
    }

    /// Moves `pane`'s thumb with the pointer at `y`; whether it scrolled.
    pub(super) fn drag_thumb(&mut self, pane: Pane, y: Pixels) -> bool {
        let handle = self.pane_scroll(pane);
        let Some((thumb, bounds, max)) = thumb(handle) else {
            return false;
        };
        let top = f32::from(y - bounds.top()) - self.thumb_grab;
        let scrolled = thumb.scrolled_at(f32::from(bounds.size.height), max, top);
        let offset = handle.offset();
        if (f32::from(offset.y) + scrolled).abs() < 0.5 {
            return false;
        }
        handle.set_offset(point(offset.x, px(-scrolled)));
        true
    }

    /// `content`, which scrolls with `pane`'s handle, with the thumb over
    /// its right edge. The thumb is the content's sibling, so it stays put
    /// while the content scrolls beneath it.
    pub(super) fn scroll_area(
        &self,
        pane: Pane,
        content: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let muted = self.theme.muted;
        let color = move |alpha: u32| rgba((muted << 8) | alpha);
        let thumb = thumb(self.pane_scroll(pane)).map(|(thumb, _, _)| {
            div()
                .id(pane.selector())
                .debug_selector(move || pane.selector().into())
                .absolute()
                .right(px(INSET))
                .top(px(thumb.left))
                .w(px(WIDTH))
                .h(px(thumb.width))
                .rounded_full()
                .bg(color(ALPHA))
                // The wheel still scrolls the pane over the thumb.
                .block_mouse_except_scroll()
                .hover(|s| s.bg(color(HOVER_ALPHA)))
                .active(|s| s.bg(color(HELD_ALPHA)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.grab_thumb(pane, event.position.y);
                    }),
                )
                .on_drag(PaneThumb(pane), |_, _, _, cx| cx.new(|_| EmptyView))
        });
        div()
            .id(match pane {
                Pane::Sections => "settings-sections-area",
                Pane::Body => "settings-body-area",
            })
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<PaneThumb>, _, cx| {
                    if event.drag(cx).0 == pane && this.drag_thumb(pane, event.event.position.y) {
                        cx.notify();
                    }
                }),
            )
            .child(content)
            .children(thumb)
    }
}
