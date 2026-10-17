//! Pages beneath the window's own drawing, on macOS.
//!
//! GPUI draws the whole window into one layer, which belongs to its view.
//! wry adds a page as a subview of that view, and AppKit draws a subview
//! above its parent's layer. So a page used to cover everything the window
//! drew, its menus and toasts included. Here each page moves into the
//! window's content view, beneath GPUI's view. GPUI's layer is transparent
//! where nothing paints, and the window leaves each page's rectangle
//! unpainted ([`crate::browser::underlay`]), so the page shows there, under
//! whatever the window draws over it.
//!
//! AppKit gives a mouse event to the view its hit test finds, before GPUI
//! sees the event, and GPUI's view now lies above the page. So each page has
//! a router: a view above GPUI's view, over the page, that only answers hit
//! tests. It hands an event to the page, unless GPUI's own hit test found
//! something at the pointer that blocks the page, such as a toast, a menu,
//! or a divider's grip. Elements block what lies beneath them by calling
//! `occlude()`.
//!
//! AppKit sends a page its mouse moves only where its hit test finds the
//! page, but tells the page the pointer left only once it leaves the page's
//! area. So a pointer that goes under something the window draws over the
//! page leaves the page hovering what lay under it last, and hovering
//! whatever its layout moves there next, as when a divider's drag narrows
//! it. The router tells the page the pointer left, as AppKit would.
//!
//! AppKit finds a drag's target with a hit test of its own, which ignores
//! the router and passes over GPUI's view, as it takes no drags, to the page
//! beneath. During a drag from another app, GPUI gets no pointer events, so
//! its hit test cannot decide there. A menu or dialog covers the whole
//! window, though: while one is open, the router takes the page's drag types
//! and refuses every drag, so a file dropped on the menu never reaches the
//! page. A toast or the file-transfer card covers only part of a page, and a
//! file dropped on one still reaches the page beneath.
//!
//! Keys go to the first responder, which a click on the page makes the page,
//! as before.
//!
//! Four parts need `unsafe`, as `objc2-app-kit` has no safe form for them:
//! reading a view's superview, borrowing GPUI's view from its window handle,
//! defining the router's class, and handing a page an exit event.
#![allow(unsafe_code)]

use {
    crate::{Error, Result},
    gpui::{Bounds, DispatchPhase, Hitbox, MouseMoveEvent, Pixels, Point, Window, point, px},
    objc2::{
        DefinedClass, MainThreadMarker, MainThreadOnly, Message as _, define_class, msg_send,
        rc::Retained,
        runtime::{NSObject, NSObjectProtocol, ProtocolObject, Sel},
        sel,
    },
    objc2_app_kit::{
        NSAutoresizingMaskOptions, NSDragOperation, NSDraggingDestination, NSDraggingInfo, NSEvent,
        NSEventType, NSResponder, NSTrackingAreaOptions, NSView, NSWindowOrderingMode,
    },
    objc2_foundation::{NSPoint, NSProcessInfo, NSRect, NSSize},
    std::cell::Cell,
    wry::{
        WebViewExtMacOS as _,
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
    },
};

/// What GPUI's hit test found at the pointer, as a page last heard it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Seen {
    /// Where GPUI last saw the pointer, in the window's points from its top
    /// left. `None` until it has.
    at: Option<Point<Pixels>>,
    /// Whether the pointer there reached the page, rather than something the
    /// window drew over it.
    reached: bool,
}

/// How far apart two pointer positions may be and still be one spot.
const SAME_SPOT: Pixels = px(0.5);

/// Whether a mouse event at `point`, in the window's points, goes to the page
/// `seen` describes. Where GPUI saw the pointer, its hit test decides.
/// Elsewhere, as after a drag inside the page that GPUI never saw, nothing of
/// the window's is known to lie over the page.
fn routes(seen: Seen, point: Point<Pixels>) -> bool {
    match seen.at {
        Some(at) if (at.x - point.x).abs() <= SAME_SPOT && (at.y - point.y).abs() <= SAME_SPOT => {
            seen.reached
        }
        _ => true,
    }
}

/// Whether the pointer just went under something the window draws over the
/// page, without leaving the page's area: GPUI's hit test reached the page
/// where it was, `was`, and no longer does, though the pointer is `inside`
/// the page.
fn went_under(was: Seen, reached: bool, inside: bool) -> bool {
    was.reached && !reached && inside
}

/// `bounds`, in the window's points from its top left, as a frame in a
/// superview `height` points tall.
fn frame(bounds: Bounds<Pixels>, height: f64, flipped: bool) -> NSRect {
    let top = f64::from(bounds.origin.y);
    let size = NSSize::new(f64::from(bounds.size.width), f64::from(bounds.size.height));
    let y = if flipped {
        top
    } else {
        height - top - size.height
    };
    NSRect::new(NSPoint::new(f64::from(bounds.origin.x), y), size)
}

/// `at`, in a superview `height` points tall, in the window's points from its
/// top left.
fn window_point(at: NSPoint, height: f64, flipped: bool) -> Point<Pixels> {
    let y = if flipped { at.y } else { height - at.y };
    point(px(at.x as f32), px(y as f32))
}

pub(crate) struct RouterIvars {
    /// The page's own view, which the router hands events to.
    page: Retained<NSView>,
    /// GPUI's view, which takes the keyboard back from the page.
    gpui: Retained<NSView>,
    seen: Cell<Seen>,
    /// Whether the router takes the page's drag types, to refuse them.
    refusing: Cell<bool>,
}

define_class!(
    /// A page's router: a view over the page, above GPUI's view, that only
    /// answers hit tests, and refuses drags while a menu covers the page. It
    /// draws nothing and takes no other events itself.
    // SAFETY: NSView may be subclassed on the main thread, which
    // `MainThreadOnly` holds every use to, and `Router` does not implement
    // `Drop`.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrPageRouter"]
    #[ivars = RouterIvars]
    pub(crate) struct Router;

    impl Router {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
            self.target(point)
        }
    }

    unsafe impl NSObjectProtocol for Router {}

    // AppKit asks these only while the router takes drag types, which it does
    // only to refuse them.
    unsafe impl NSDraggingDestination for Router {
        #[unsafe(method(draggingEntered:))]
        fn dragging_entered(&self, _: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            NSDragOperation::None
        }

        #[unsafe(method(draggingUpdated:))]
        fn dragging_updated(&self, _: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            NSDragOperation::None
        }

        #[unsafe(method(prepareForDragOperation:))]
        fn prepare_for_drag_operation(&self, _: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            false
        }

        #[unsafe(method(performDragOperation:))]
        fn perform_drag_operation(&self, _: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            false
        }
    }
);

impl Router {
    /// The view of the page under `point`, which is in the superview's
    /// coordinates, or none, so that AppKit asks GPUI's view next.
    fn target(&self, point: NSPoint) -> Option<Retained<NSView>> {
        if self.isHidden() {
            return None;
        }
        // SAFETY: a property read on this view, on the main thread.
        let superview = unsafe { self.superview() }?;
        let at = window_point(point, superview.bounds().size.height, superview.isFlipped());
        if !routes(self.ivars().seen.get(), at) {
            return None;
        }
        self.ivars().page.hitTest(point)
    }

    fn new(
        page: Retained<NSView>,
        gpui: Retained<NSView>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RouterIvars {
            page,
            gpui,
            seen: Cell::default(),
            refusing: Cell::new(false),
        });
        // SAFETY: NSView's designated initializer, on an allocated instance
        // with its ivars set.
        unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] }
    }

    /// Puts the page and its router at `bounds`, in the window's points.
    pub(crate) fn place(&self, bounds: Bounds<Pixels>) {
        // SAFETY: a property read on this view, on the main thread.
        let Some(superview) = (unsafe { self.superview() }) else {
            return;
        };
        let frame = frame(
            bounds,
            superview.bounds().size.height,
            superview.isFlipped(),
        );
        let router: &NSView = self;
        for view in [router, &self.ivars().page] {
            if view.frame() != frame {
                view.setFrame(frame);
            }
        }
    }

    pub(crate) fn set_shown(&self, shown: bool) {
        self.setHidden(!shown);
    }

    /// Refuses every drag over the page, or stops refusing. While refusing,
    /// the router takes the page's drag types, so AppKit's drag hit test
    /// finds the router, above the page, first.
    pub(crate) fn refuse_drops(&self, refuse: bool) {
        let ivars = self.ivars();
        if ivars.refusing.replace(refuse) == refuse {
            return;
        }
        if refuse {
            self.registerForDraggedTypes(&ivars.page.registeredDraggedTypes());
        } else {
            self.unregisterDraggedTypes();
        }
    }

    /// Moves keyboard input into the page.
    pub(crate) fn focus(&self) {
        let page = &self.ivars().page;
        if let Some(window) = page.window() {
            let responder: &NSResponder = page;
            window.makeFirstResponder(Some(responder));
        }
    }

    /// Hands the keyboard to GPUI's view.
    pub(crate) fn blur(&self) {
        let gpui = &self.ivars().gpui;
        if let Some(window) = gpui.window() {
            let responder: &NSResponder = gpui;
            window.makeFirstResponder(Some(responder));
        }
    }

    /// Whether the page is its window's first responder, which takes keys.
    pub(crate) fn holds_keyboard(&self) -> bool {
        let page = &self.ivars().page;
        page.window()
            .and_then(|window| window.firstResponder())
            .is_some_and(|responder| std::ptr::addr_eq(&*responder, &**page))
    }

    /// Hands the keyboard to GPUI's view if the page holds it.
    pub(crate) fn release_keyboard(&self) {
        if self.holds_keyboard() {
            self.blur();
        }
    }

    pub(crate) fn remove(&self) {
        self.removeFromSuperview();
    }

    /// Follows GPUI's hit test for the page laid out at `hitbox`: as this
    /// frame paints, and at each mouse move until the next one.
    pub(crate) fn follow(this: &Retained<Self>, hitbox: Hitbox, window: &mut Window) {
        this.see(&hitbox, window);
        let router = this.clone();
        window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, _| {
            if phase == DispatchPhase::Capture {
                router.see(&hitbox, window);
            }
        });
    }

    fn see(&self, hitbox: &Hitbox, window: &Window) {
        let at = window.mouse_position();
        // Keyboard input leaves nothing hovered until the pointer moves, but
        // the pointer is still where it was.
        let reached = if window.last_input_was_keyboard() {
            hitbox.should_handle_scroll(window)
        } else {
            hitbox.is_hovered(window)
        };
        let was = self.ivars().seen.replace(Seen {
            at: Some(at),
            reached,
        });
        if went_under(was, reached, hitbox.bounds.contains(&at)) {
            self.leave();
        }
    }

    /// Tells the page the pointer left it, as AppKit does when the pointer
    /// leaves the page's area: an exit event, to the owner of each of the
    /// page's areas that tracks the pointer. The exit lies just outside the
    /// page, so the page hovers nothing until the pointer comes back.
    fn leave(&self) {
        let exited: Sel = sel!(mouseExited:);
        let page = &self.ivars().page;
        let Some(window) = page.window() else {
            return;
        };
        let outside = page.convertPoint_toView(NSPoint::new(-1., -1.), None);
        // SAFETY: AppKit's constructor for an exit event, with no user data.
        // A synthetic event has no graphics context or tracking number.
        let exit = unsafe {
            NSEvent::enterExitEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_trackingNumber_userData(
                NSEventType::MouseExited,
                outside,
                NSEvent::modifierFlags_class(),
                NSProcessInfo::processInfo().systemUptime(),
                window.windowNumber(),
                None,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        let Some(exit) = exit else {
            return;
        };
        let tracking =
            NSTrackingAreaOptions::MouseMoved | NSTrackingAreaOptions::MouseEnteredAndExited;
        for area in page.trackingAreas() {
            let Some(owner) = area.owner().filter(|_| area.options().contains(tracking)) else {
                continue;
            };
            // SAFETY: `respondsToSelector:` takes a selector and returns a
            // BOOL, on any object.
            let answers: bool = unsafe { msg_send![&*owner, respondsToSelector: exited] };
            if answers {
                // SAFETY: the owner answers `mouseExited:`, which takes one
                // event, as AppKit sends it.
                let _: () = unsafe { msg_send![&*owner, mouseExited: &*exit] };
            }
        }
    }
}

/// Moves `page` beneath GPUI's view in `window`, and puts a router for it
/// above GPUI's view. The router stays hidden until the page shows, and both
/// stay where they are until the router places them.
pub(crate) fn stack(page: &wry::WebView, window: &Window) -> Result<Retained<Router>> {
    let mtm = MainThreadMarker::new().ok_or(Error::PageStack)?;
    let gpui = gpui_view(window)?;
    // SAFETY: a property read on GPUI's view, on the main thread.
    let content = unsafe { gpui.superview() }.ok_or(Error::PageStack)?;
    let view: Retained<NSView> = Retained::into_super(Retained::into_super(page.webview()));
    view.removeFromSuperview();
    content.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, Some(&gpui));
    let router = Router::new(view, gpui.clone(), mtm);
    router.setHidden(true);
    // Pinned to the window's top while it resizes, as wry pins the page.
    router.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    content.addSubview_positioned_relativeTo(&router, NSWindowOrderingMode::Above, Some(&gpui));
    Ok(router)
}

/// GPUI's view in `window`.
fn gpui_view(window: &Window) -> Result<Retained<NSView>> {
    // Window has an inherent `window_handle` of its own.
    let handle = HasWindowHandle::window_handle(window)?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(Error::PageStack);
    };
    // SAFETY: the handle borrows `window`, whose GPUI view is a live NSView
    // for as long as that borrow, and the view is retained before it ends.
    // `stack` checked that this is the main thread.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    Ok(view.retain())
}

#[cfg(test)]
mod tests;
