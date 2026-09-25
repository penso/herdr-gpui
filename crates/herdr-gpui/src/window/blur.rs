//! Tint-free macOS backdrop blur through the window server's private radius API.
//! No AppKit visual-effect material is inserted: its tint obscures the desktop.
#![allow(unsafe_code)] // Narrow runtime-loaded macOS ABI and GPUI's borrowed NSView pointer.
use gpui::Window;
use libloading::os::unix::Library;
use objc2::rc::Retained;
use objc2_app_kit::NSView;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::OnceLock;

// SkyLight exports these process-wide symbols on supported macOS versions. If
// either disappears in a system update, blur is simply unavailable; no call is
// made through an unresolved pointer. This private ABI is used only on macOS.
struct BlurSymbols {
    _library: Library,
    connection: unsafe extern "C" fn() -> u32,
    set_radius: unsafe extern "C" fn(u32, u32, i32) -> i32,
}

fn symbols() -> Option<&'static BlurSymbols> {
    static SYMBOLS: OnceLock<Option<BlurSymbols>> = OnceLock::new();
    SYMBOLS
        .get_or_init(|| {
            // `this()` looks up symbols already loaded by AppKit, without loading
            // another framework or doing disk I/O on GPUI's UI thread.
            let library = Library::this();
            // SAFETY: Each function pointer is copied only after symbol lookup;
            // `_library` keeps its process handle alive for all subsequent calls.
            let connection = unsafe { *library.get(b"CGSDefaultConnectionForThread\0").ok()? };
            let set_radius = unsafe { *library.get(b"CGSSetWindowBackgroundBlurRadius\0").ok()? };
            Some(BlurSymbols {
                _library: library,
                connection,
                set_radius,
            })
        })
        .as_ref()
}

pub(crate) fn available() -> bool {
    symbols().is_some()
}

/// Linear 0–100 slider mapped to window-server radius 0–60. The blur is a
/// background operation and never adds a solid colour to the window.
fn radius(strength: u8) -> i32 {
    (u32::from(strength.min(100)) * 60).div_ceil(100) as i32
}

pub(crate) struct NativeBlur {
    window_number: u32,
    radius: i32,
    failed: bool,
}

impl NativeBlur {
    pub(super) fn update(current: &mut Option<Self>, window: &Window, strength: u8) {
        if !available() {
            return;
        }
        let Some(symbols) = symbols() else {
            return;
        };
        let requested = radius(strength);
        if requested == 0 && current.is_none() {
            return;
        }
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return; // GPUI's headless test window has no native NSView.
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: GPUI returns a live NSView for this Window on its UI thread;
        // retain it for the duration of the AppKit window lookup.
        let Some(renderer) =
            (unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) })
        else {
            return;
        };
        let Some(number) = renderer
            .window()
            .and_then(|window| u32::try_from(window.windowNumber()).ok())
        else {
            return;
        };
        if number == 0 {
            return;
        }
        if let Some(previous) = current.as_ref()
            && previous.window_number == number
            && (previous.radius == requested || previous.failed)
        {
            return;
        }
        // SAFETY: Both pointers were resolved from the loaded SkyLight image;
        // the window number came from AppKit for this live window. GPUI calls us
        // on the UI thread, and the server validates connection/window/radius.
        let result = unsafe { (symbols.set_radius)((symbols.connection)(), number, requested) };
        if result != 0 {
            tracing::warn!(
                result,
                "macOS background blur radius unavailable; leaving blur off"
            );
        }
        *current = Some(Self {
            window_number: number,
            radius: requested,
            failed: result != 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn blur_strength_has_bounded_radius_and_zero_is_off() {
        assert_eq!(radius(0), 0);
        assert_eq!(radius(1), 1);
        assert_eq!(radius(50), 30);
        assert_eq!(radius(100), 60);
        assert_eq!(radius(255), 60);
    }
}
