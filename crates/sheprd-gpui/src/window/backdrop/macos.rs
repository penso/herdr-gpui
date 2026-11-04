//! The main window's surface on macOS.
//!
//! GPUI draws transparent wherever nothing paints, whatever the appearance,
//! so that pages beneath its view show where the window leaves them
//! unpainted (`browser::stacking`). GPUI does that only in its `Transparent`
//! appearance, which also makes the native window transparent. So for an
//! opaque appearance the window is made opaque again, filled with the
//! theme's background wherever nothing covers it.
//!
//! GPUI's `Blurred` appearance puts an `NSVisualEffectView` behind its
//! surface and strips that view's layers of their tint and saturation. On
//! macOS 26 and later that leaves the desktop unblurred: the window looks
//! the same as `Transparent`. The window server's own per-window blur, which
//! WezTerm, Ghostty, and kitty use, still works, so the window sets the blur
//! itself.
//!
//! The window server's call has no public or safe binding, and reaching the
//! `NSWindow` behind GPUI's handle means dereferencing the raw view pointer,
//! so both need `unsafe`.
#![allow(unsafe_code)]

use gpui::{Window, WindowBackgroundAppearance};
use objc2::rc::Retained;
use objc2_app_kit::{NSColor, NSWindow};
use std::ffi::c_int;
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// How far the desktop is blurred, in points.
const RADIUS: c_int = 24;

// Exported by CoreGraphics, from the window server's SkyLight framework. No
// public API blurs what lies behind a window, and the AppKit view that should
// is what fails above.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSDefaultConnectionForThread() -> c_int;
    fn CGSSetWindowBackgroundBlurRadius(connection: c_int, window: isize, radius: c_int) -> c_int;
}

/// What the window was last given: an appearance, and the theme's
/// background, which an opaque window fills with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Surface {
    pub(super) appearance: WindowBackgroundAppearance,
    pub(super) ground: u32,
}

/// Moves `window` from the surface `was`, if it had one yet, to `surface`.
pub(super) fn apply(window: &mut Window, was: Option<Surface>, surface: Surface) {
    use WindowBackgroundAppearance::{Blurred, Opaque, Transparent};
    window.set_background_appearance(Transparent);
    let blurred = surface.appearance == Blurred;
    if blurred || was.is_some_and(|was| was.appearance == Blurred) {
        blur(window, blurred);
    }
    if surface.appearance == Opaque {
        fill(window, surface.ground);
    }
}

/// Makes `window` opaque to the window server, filled with `ground` where
/// nothing covers it.
fn fill(window: &Window, ground: u32) {
    let Some(native) = native_window(window) else {
        tracing::warn!("Could not find the native window to make opaque");
        return;
    };
    let channel = |shift: u32| f64::from((ground >> shift) & 0xff) / 255.;
    let color = NSColor::colorWithSRGBRed_green_blue_alpha(channel(16), channel(8), channel(0), 1.);
    native.setOpaque(true);
    native.setBackgroundColor(Some(&color));
}

/// Blurs the desktop behind `window`, or stops blurring it.
fn blur(window: &Window, blur: bool) {
    let Some(number) = native_window(window).map(|native| native.windowNumber()) else {
        tracing::warn!("Could not find the native window to blur behind");
        return;
    };
    // SAFETY: both functions take plain integers and touch no memory of
    // ours. The connection is this thread's own, and `number` names a window
    // this process owns, as the window server checks.
    let status = unsafe {
        CGSSetWindowBackgroundBlurRadius(
            CGSDefaultConnectionForThread(),
            number,
            if blur { RADIUS } else { 0 },
        )
    };
    if status != 0 {
        tracing::warn!(status, "The window server refused the backdrop blur");
    }
}

/// The `NSWindow` behind `window`.
fn native_window(window: &Window) -> Option<Retained<NSWindow>> {
    // Window has an inherent `window_handle` of its own.
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // AppKit views may only be used on the main thread, where GPUI renders.
    objc2::MainThreadMarker::new()?;
    // SAFETY: the handle borrows `window`, whose GPUI view is alive and an
    // `NSView` for as long as that borrow, and the check above holds us to
    // the main thread.
    let view = unsafe { handle.ns_view.cast::<objc2_app_kit::NSView>().as_ref() };
    view.window()
}
