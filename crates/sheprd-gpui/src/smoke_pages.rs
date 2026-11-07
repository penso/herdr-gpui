//! Native check of pages beneath the window's drawing, in the daemon-free
//! fixture window. A real page in the right of two groups must lie below
//! GPUI's view, where it was laid out, with its router above GPUI's view.
//! AppKit's hit test must hand a click on the page to the page, and a click
//! on the divider's grip over the page's edge to GPUI. AppKit's drag hit test
//! must reach the page. GPUI's layer must be transparent in an opaque window.
//! A page under the pointer must stop hovering once the pointer moves onto
//! the grip, where AppKit tells the page nothing, and hover again once the
//! pointer is back. While a dialog covers the window, AppKit's drag hit test
//! must not reach the page, and must reach it again once the dialog closes.
//!
//! The page comes from this process, on the loopback address, so nothing
//! leaves the machine. WebKit ignores the pointer in an inactive window, so
//! the check activates its own, which macOS may refuse while another app is
//! in use.
//!
//! Checking needs AppKit calls with no safe binding: reading a view's
//! superview and layer, synthetic mouse moves delivered to GPUI's view or to
//! the page's pointer tracking alone, and AppKit's private drag hit test,
//! which only this check calls. Reading what the page hovers runs a script in
//! it, which answers through a callback.
#![allow(unsafe_code)]

use super::page_fixture::{open_page, page_url, serve};
use super::*;
use crate::browser::TabId;
use objc2::{
    ClassType, Message as _, class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObjectProtocol as _},
};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSTrackingAreaOptions, NSView};
use objc2_foundation::{NSError, NSPoint, NSString};
use objc2_web_kit::WKWebView;
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Asks the page whether the pointer hovers its one element.
const HOVERED: &str = "String(document.querySelector('#spot').matches(':hover'))";

/// How long WebKit may take to load the page or to answer the pointer, and
/// macOS to activate the window.
const SETTLE: Duration = Duration::from_secs(3);

/// The views the check reads, and where it points, in the content view's
/// coordinates.
struct Native {
    gpui: Retained<NSView>,
    content: Retained<NSView>,
    page: Retained<NSView>,
    window_number: isize,
    /// The page's middle.
    center: NSPoint,
    /// On the divider's grip, over the page's left edge.
    grip: NSPoint,
}

pub(super) fn start_pages(handle: WindowHandle<SheprdWindow>, cx: &mut App) {
    EXIT_CODE.store(1, Ordering::SeqCst);
    let timer = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let result = async {
            let port = serve()?;
            let id = handle
                .update(cx, |view, window, cx| {
                    open_page(view, window, cx, Some(page_url(port)?)).map(|(_, id)| id)
                })
                .map_err(|error| anyhow!("{error:#}"))??;
            // Let the page show, lay out, and paint a few frames.
            for _ in 0..10 {
                timer.timer(Duration::from_millis(100)).await;
                AnyWindowHandle::from(handle)
                    .update(cx, |_, window, cx| window.draw(cx).clear(cx))
                    .map_err(|error| anyhow!("{error:#}"))?;
            }
            let native = handle
                .update(cx, |view, window, _| locate(view, window, id))
                .map_err(|error| anyhow!("{error:#}"))??;
            // Outside every GPUI borrow, as GPUI handles the mouse move
            // there and then.
            let placed = check(&native)?;
            let hovered = hover(&native, handle, cx, &timer).await?;
            let dropped = drops(&native, handle, cx, &timer).await?;
            Ok::<_, anyhow::Error>(format!("{placed}; {hovered}; {dropped}"))
        }
        .await;
        match result {
            Ok(summary) => {
                eprintln!("PAGES native PASS: {summary}");
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!("PAGES native FAIL: {error:#}");
                std::process::exit(1);
            }
        }
    })
    .detach();
}

fn locate(view: &SheprdWindow, window: &Window, id: TabId) -> Result<Native> {
    let page_at = view
        .browser
        .drawn_pages()
        .borrow()
        .get(&id)
        .copied()
        .context("the page never laid out")?;
    let right = view
        .group_slots()
        .get(1)
        .and_then(|slot| view.browser.group_bounds.borrow().get(&slot.id).copied())
        .context("the right group never drew")?;
    let handle = HasWindowHandle::window_handle(window)?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        bail!("not an AppKit window");
    };
    // SAFETY: the handle borrows `window`, whose GPUI view is a live NSView
    // for that borrow, retained before it ends, on the main thread.
    let gpui = unsafe { handle.ns_view.cast::<NSView>().as_ref() }.retain();
    // SAFETY: a property read on GPUI's view, on the main thread.
    let content = unsafe { gpui.superview() }.context("GPUI's view has no superview")?;
    let page = content
        .subviews()
        .iter()
        .find(|view| view.isKindOfClass(WKWebView::class()))
        .context("no page in the content view")?;
    let window_number = gpui
        .window()
        .context("GPUI's view has no window")?
        .windowNumber();
    let height = content.bounds().size.height;
    let appkit = |at: Point<Pixels>| NSPoint::new(f64::from(at.x), height - f64::from(at.y));
    // A grip point off the page would reach GPUI without the router.
    let grip = point(right.left() + px(1.), page_at.center().y);
    if !page_at.contains(&grip) {
        bail!("the grip at {grip:?} does not lie over the page at {page_at:?}");
    }
    Ok(Native {
        center: appkit(page_at.center()),
        grip: appkit(grip),
        gpui,
        content,
        page,
        window_number,
    })
}

fn check(native: &Native) -> Result<String> {
    let subviews = native.content.subviews();
    let index = |target: &NSView| {
        subviews
            .iter()
            .position(|view| std::ptr::eq(&*view, target))
    };
    let (Some(page), Some(gpui)) = (index(&native.page), index(&native.gpui)) else {
        bail!("the page and GPUI's view are not siblings");
    };
    if page > gpui {
        bail!("the page lies above GPUI's view");
    }
    let router = subviews
        .iter()
        .position(|view| view.class().name() == c"SheprdPageRouter")
        .context("the page has no router")?;
    if router < gpui {
        bail!("the router lies beneath GPUI's view");
    }
    // SAFETY: a property read on GPUI's view, on the main thread.
    let layer: Option<Retained<AnyObject>> = unsafe { msg_send![&*native.gpui, layer] };
    let layer = layer.context("GPUI's view has no layer")?;
    // SAFETY: `isOpaque` is a BOOL property of every CALayer.
    let layer_opaque: bool = unsafe { msg_send![&*layer, isOpaque] };
    if layer_opaque {
        bail!("GPUI's layer is opaque, so it hides the page");
    }
    if !native.gpui.window().is_some_and(|window| window.isOpaque()) {
        bail!("the opaque window lets the desktop through");
    }
    point_at(native, native.center)?;
    let on_page = native.content.hitTest(native.center);
    if !on_page.is_some_and(|view| view.isDescendantOf(&native.page)) {
        bail!("a click on the page does not reach it");
    }
    point_at(native, native.grip)?;
    let on_grip = native.content.hitTest(native.grip);
    if !on_grip.is_some_and(|view| std::ptr::eq(&*view, &*native.gpui)) {
        bail!("a click on the divider's grip over the page does not reach GPUI");
    }
    if !drag_reaches_page(native) {
        bail!("a drag over the page does not reach it");
    }
    Ok(format!(
        "page {page} < GPUI view {gpui} < router {router}; GPUI's layer transparent in an opaque window; page click, grip click, and drag each reach their target"
    ))
}

/// Whether AppKit's drag hit test, for the drag types the page takes, finds
/// the page at its middle.
fn drag_reaches_page(native: &Native) -> bool {
    let types = native.page.registeredDraggedTypes();
    // SAFETY: `setWithArray:` takes an array and returns a new set.
    let types: Retained<AnyObject> = unsafe { msg_send![class!(NSSet), setWithArray: &*types] };
    let mut at = native.center;
    // SAFETY: AppKit's own drag hit test on NSView, which takes a point in
    // the receiver's superview's coordinates by reference and a set of
    // pasteboard types, and returns a view or nil.
    let found: *mut AnyObject = unsafe {
        msg_send![&*native.content, _hitTest: &mut at as *mut NSPoint, dragTypes: &*types]
    };
    // SAFETY: AppKit returns a live view in the window, or nil.
    let found = unsafe { found.cast::<NSView>().as_ref() };
    found.is_some_and(|view| view.isDescendantOf(&native.page))
}

/// Opens the command palette, a dialog over the whole window, and checks
/// that a drag over the page no longer reaches it; then closes the palette
/// and checks that a drag reaches the page again.
async fn drops(
    native: &Native,
    handle: WindowHandle<SheprdWindow>,
    cx: &mut AsyncApp,
    timer: &BackgroundExecutor,
) -> Result<&'static str> {
    handle
        .update(cx, |view, window, cx| {
            view.command(Command::Palette, window, cx)
        })
        .map_err(|error| anyhow!("{error:#}"))?;
    draw_frames(handle, cx, timer).await?;
    if drag_reaches_page(native) {
        bail!("a drag over a page under the palette reaches the page");
    }
    handle
        .update(cx, |view, window, cx| view.dismiss_menu(window, cx))
        .map_err(|error| anyhow!("{error:#}"))?;
    draw_frames(handle, cx, timer).await?;
    if !drag_reaches_page(native) {
        bail!("a drag over the page does not reach it once the palette closes");
    }
    Ok("a drag over the page is refused under a dialog, and reaches it again after")
}

/// Lets the window draw a few frames, which present the pages.
async fn draw_frames(
    handle: WindowHandle<SheprdWindow>,
    cx: &mut AsyncApp,
    timer: &BackgroundExecutor,
) -> Result<()> {
    for _ in 0..3 {
        timer.timer(Duration::from_millis(50)).await;
        AnyWindowHandle::from(handle)
            .update(cx, |_, window, cx| window.draw(cx).clear(cx))
            .map_err(|error| anyhow!("{error:#}"))?;
    }
    Ok(())
}

/// Moves the pointer over the page, onto the divider's grip, and back, as
/// AppKit hands each move out, and checks what the page hovers after each.
async fn hover(
    native: &Native,
    handle: WindowHandle<SheprdWindow>,
    cx: &mut AsyncApp,
    timer: &BackgroundExecutor,
) -> Result<&'static str> {
    cx.update(|cx| cx.activate(true));
    handle
        .update(cx, |_, window, _| window.activate_window())
        .map_err(|error| anyhow!("{error:#}"))?;
    let window = native.gpui.window().context("GPUI's view has no window")?;
    let deadline = Instant::now() + SETTLE;
    while !window.isKeyWindow() {
        if Instant::now() >= deadline {
            bail!(
                "the window never became active, and WebKit ignores the pointer in an inactive window: rerun with no other app in use"
            );
        }
        timer.timer(Duration::from_millis(50)).await;
    }
    let deadline = Instant::now() + SETTLE;
    while hovered(native, timer).await.is_none() {
        if Instant::now() >= deadline {
            bail!("the page never loaded");
        }
        timer.timer(Duration::from_millis(50)).await;
    }
    // Over the page, AppKit tells both GPUI and the page.
    point_at(native, native.center)?;
    track(native, native.center)?;
    settle(native, timer, true)
        .await
        .context("the page under the pointer is not hovered")?;
    // On the grip over the page's edge, AppKit tells GPUI alone.
    point_at(native, native.grip)?;
    settle(native, timer, false)
        .await
        .context("the page goes on hovering under the divider's grip")?;
    point_at(native, native.center)?;
    track(native, native.center)?;
    settle(native, timer, true)
        .await
        .context("the page does not hover again once the pointer is back")?;
    Ok("the page stops hovering under the grip and hovers again past it")
}

/// Waits for the page to report that the pointer hovers it, or not, as
/// `hovering` says.
async fn settle(native: &Native, timer: &BackgroundExecutor, hovering: bool) -> Result<()> {
    let deadline = Instant::now() + SETTLE;
    loop {
        let now = hovered(native, timer).await;
        if now == Some(hovering) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("the page reports {now:?}");
        }
        timer.timer(Duration::from_millis(50)).await;
    }
}

/// Whether the pointer hovers the page, or `None` before the page loads.
async fn hovered(native: &Native, timer: &BackgroundExecutor) -> Option<bool> {
    let page = native.page.downcast_ref::<WKWebView>()?;
    let (sender, answer) = std::sync::mpsc::channel();
    let done = block2::RcBlock::new(move |result: *mut AnyObject, _: *mut NSError| {
        // SAFETY: WebKit hands the script's result, a live object, or nil.
        let result = unsafe { result.as_ref() };
        let text = result
            .and_then(|result| result.downcast_ref::<NSString>())
            .map(NSString::to_string);
        let _ = sender.send(text);
    });
    // SAFETY: WebKit runs the script in the page and calls `done` once, on
    // the main thread, with its result or an error.
    unsafe { page.evaluateJavaScript_completionHandler(&NSString::from_str(HOVERED), Some(&done)) };
    let deadline = Instant::now() + SETTLE;
    loop {
        if let Ok(text) = answer.try_recv() {
            return text.and_then(|text| text.parse().ok());
        }
        if Instant::now() >= deadline {
            return None;
        }
        timer.timer(Duration::from_millis(10)).await;
    }
}

/// A mouse move to `at`, in the window.
fn mouse_move(native: &Native, at: NSPoint) -> Result<Retained<NSEvent>> {
    // SAFETY: the documented constructor for a mouse event, with no
    // graphics context, as AppKit asks.
    let event: Option<Retained<NSEvent>> = unsafe {
        msg_send![
            NSEvent::class(),
            mouseEventWithType: NSEventType::MouseMoved,
            location: at,
            modifierFlags: NSEventModifierFlags::empty(),
            timestamp: 0_f64,
            windowNumber: native.window_number,
            context: None::<&AnyObject>,
            eventNumber: 0_isize,
            clickCount: 0_isize,
            pressure: 0_f32
        ]
    };
    event.context("no mouse event")
}

/// Tells GPUI the pointer is at `at`, as a mouse move to its view alone
/// would: nothing is posted to the system, and the real pointer stays.
fn point_at(native: &Native, at: NSPoint) -> Result<()> {
    native.gpui.mouseMoved(&*mouse_move(native, at)?);
    Ok(())
}

/// Tells the page the pointer is at `at`, as AppKit does, through the owner
/// of the page's area that tracks mouse moves.
fn track(native: &Native, at: NSPoint) -> Result<()> {
    let event = mouse_move(native, at)?;
    let owner = native
        .page
        .trackingAreas()
        .iter()
        .find(|area| area.options().contains(NSTrackingAreaOptions::MouseMoved))
        .and_then(|area| area.owner())
        .context("the page tracks no mouse moves")?;
    // SAFETY: AppKit sends the owner of an area that tracks mouse moves
    // `mouseMoved:` with one event.
    let _: () = unsafe { msg_send![&*owner, mouseMoved: &*event] };
    Ok(())
}
