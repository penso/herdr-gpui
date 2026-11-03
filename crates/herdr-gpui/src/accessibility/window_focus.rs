//! Makes a window answer `AXFocusedUIElement` with the focused AccessKit node.
//!
//! GPUI makes an inner view the first responder but attaches AccessKit to the
//! window's content view, so AppKit answers with the window itself. Dictation
//! tools such as Typeless then find no text field to type into and fall back to
//! asking the user to paste.

// Adding a method to GPUI's window classes needs the Objective-C runtime.
#![allow(unsafe_code)]

use objc2::{runtime::AnyClass, sel};
use std::{ffi::CStr, sync::Once};

/// The classes GPUI's macOS backend registers for its windows and panels.
const WINDOW_CLASSES: [&CStr; 2] = [c"GPUIWindow", c"GPUIPanel"];

/// Forwards each GPUI window class's focused element to its content view.
/// Runs once; later calls do nothing.
pub(crate) fn install() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        for name in WINDOW_CLASSES {
            let Some(class) = AnyClass::get(name) else {
                tracing::warn!(class = ?name, "GPUI window class not registered");
                continue;
            };
            if defines_focus(class) {
                continue;
            }
            let Ok(name) = name.to_str() else {
                continue;
            };
            // SAFETY: the class exists and is GPUI's NSWindow or NSPanel subclass,
            // which does not define the selector itself, so the adapter neither
            // panics nor replaces a method. The adapter is statically linked and
            // never unloaded, so the added method stays valid.
            unsafe { accesskit_macos::add_focus_forwarder_to_window_class(name) };
        }
    });
}

/// Whether the class itself, not a superclass, defines the focused element.
fn defines_focus(class: &AnyClass) -> bool {
    class
        .instance_methods()
        .iter()
        .any(|method| method.name() == sel!(accessibilityFocusedUIElement))
}

#[cfg(test)]
mod tests;
