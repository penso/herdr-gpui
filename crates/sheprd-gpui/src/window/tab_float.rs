//! On macOS, a tab carried out of the window is drawn under the pointer in
//! a small window of its own, as a browser draws a tab dragged off its
//! window. GPUI cannot move a window once open, so the card's window is
//! moved through AppKit. It watches the window the tab came from: it
//! follows the pointer while the drag is out, hides while it is back in,
//! and closes once the drag ends or that window goes.

use super::{
    SheprdWindow,
    tab_ghost::{Card, MAX_WIDTH, OFFSET},
};
use gpui::{prelude::*, *};
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow};
use objc2_foundation::{NSPoint, NSRect};
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Room around the card for its shadow, which the window must hold.
const PAD: f32 = 12.;
/// Tall enough for a label and the line under it.
const HEIGHT: f32 = 72.;

/// How far the window carrying a tab's card has got, for one drag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Float {
    #[default]
    Unopened,
    Opening,
    Open,
    /// It could not open, so the drag's window draws the card at its edge
    /// instead, rather than trying again on every pointer move.
    Failed,
}

/// The window drawing a carried tab's card.
pub(crate) struct TabFloat {
    card: Card,
    source: Retained<NSWindow>,
    own: Option<Retained<NSWindow>>,
    _follow: [Subscription; 2],
}

impl SheprdWindow {
    /// Opens the window that carries the tab's card once the drag first
    /// leaves this window. After that the card's window follows on its own.
    pub(super) fn float_tab(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(ghost) = self.tab_ghost().filter(|ghost| ghost.outside) else {
            return;
        };
        if self
            .tab_drag
            .as_ref()
            .is_none_or(|drag| drag.float != Float::Unopened)
        {
            return;
        }
        let (Some(card), Some(source)) = (self.ghost_card(&ghost, cx), ns_window(window)) else {
            self.settle_float(Float::Failed, cx);
            return;
        };
        self.settle_float(Float::Opening, cx);
        let from = cx.entity();
        let back = from.downgrade();
        cx.defer(move |cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Point::default(),
                    size(px(MAX_WIDTH + 2. * PAD), px(HEIGHT)),
                ))),
                titlebar: None,
                focus: false,
                show: false,
                kind: WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            let opened = cx.open_window(options, |window, cx| {
                cx.new(|cx| TabFloat::new(card, source, from, window, cx))
            });
            let float = match opened {
                Ok(_) => Float::Open,
                Err(error) => {
                    tracing::warn!(%error, "Cannot open the window for a carried tab");
                    Float::Failed
                }
            };
            // A drag already over has no card to draw.
            let _ = back.update(cx, |this, cx| {
                if this
                    .tab_drag
                    .as_ref()
                    .is_some_and(|drag| drag.float == Float::Opening)
                {
                    this.settle_float(float, cx);
                }
            });
        });
    }

    fn settle_float(&mut self, float: Float, cx: &mut Context<Self>) {
        if let Some(drag) = self.tab_drag.as_mut() {
            drag.float = float;
            cx.notify();
        }
    }
}

impl TabFloat {
    fn new(
        card: Card,
        source: Retained<NSWindow>,
        from: Entity<SheprdWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let own = ns_window(window);
        if let Some(own) = &own {
            // The source window keeps the pointer for the drop; the card
            // never takes a click, and draws its own shadow.
            own.setIgnoresMouseEvents(true);
            own.setHasShadow(false);
        }
        let mut float = Self {
            card,
            source,
            own,
            _follow: [
                cx.observe_in(&from, window, |this, from, window, cx| {
                    this.follow(from.read(cx), window);
                }),
                cx.observe_release_in(&from, window, |_, _, window, _| window.remove_window()),
            ],
        };
        float.follow(from.read(cx), window);
        float
    }

    /// Moves the card under the pointer while the tab is carried out of
    /// `from`, hides it while back in, and closes once the drag is over.
    fn follow(&mut self, from: &SheprdWindow, window: &mut Window) {
        let Some(own) = &self.own else {
            window.remove_window();
            return;
        };
        let Some(drag) = from.tab_drag.as_ref().filter(|drag| drag.floating()) else {
            window.remove_window();
            return;
        };
        if !drag.outside {
            own.orderOut(None);
            return;
        }
        let content = self.source.contentRectForFrameRect(self.source.frame());
        own.setFrameTopLeftPoint(top_left(content, drag.pointer));
        own.orderFrontRegardless();
    }
}

impl Render for TabFloat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(PAD))
            .flex()
            .items_start()
            .child(self.card.render())
    }
}

/// Where the card's window puts its top left, in AppKit's screen points,
/// which count up from the bottom: the card just below and right of
/// `pointer`, a point in the source window's `content`, which counts down
/// from its top.
fn top_left(content: NSRect, pointer: Point<Pixels>) -> NSPoint {
    let x = f64::from(f32::from(pointer.x) + OFFSET - PAD);
    let y = f64::from(f32::from(pointer.y) + OFFSET - PAD);
    NSPoint::new(
        content.origin.x + x,
        content.origin.y + content.size.height - y,
    )
}

/// The AppKit window behind `window`.
#[allow(unsafe_code)]
fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
    // Window has an inherent `window_handle` of its own.
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: GPUI's handle names the window's content view, which the
    // window retains while it is open; it is only read here, on the main
    // thread the window lives on, and the result is retained by `window()`.
    // No safe accessor reaches the `NSWindow` from GPUI.
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    view.window()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use objc2_foundation::NSSize;

    #[test]
    fn the_card_sits_below_and_right_of_the_pointer_in_screen_points() {
        // A window 800 by 600 whose content starts 100 points from the
        // screen's left and 200 from its bottom.
        let content = NSRect::new(NSPoint::new(100., 200.), NSSize::new(800., 600.));
        let at = |x: f32, y: f32| top_left(content, point(px(x), px(y)));
        let shift = f64::from(OFFSET - PAD);
        assert_eq!(at(0., 0.), NSPoint::new(100. + shift, 800. - shift));
        // Past the right and bottom edges, the card keeps following.
        assert_eq!(at(900., 700.), NSPoint::new(1000. + shift, 100. - shift));
        // Above and left of the window too.
        assert_eq!(at(-50., -40.), NSPoint::new(50. + shift, 840. - shift));
    }
}
