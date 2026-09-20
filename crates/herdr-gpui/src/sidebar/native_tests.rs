//! Deliver clicks only to the fixture's own AppKit content view.
#![allow(unsafe_code, deprecated, unexpected_cfgs)]
use cocoa::{
    appkit::{NSApp, NSView, NSWindow},
    base::{id, nil},
    foundation::NSPoint,
};
use objc::{class, msg_send, sel, sel_impl};

pub(crate) fn click(x: f64, y: f64) -> Result<(), String> {
    unsafe {
        // Match the performance adapter: a fixture can render without being key.
        let windows: id = msg_send![NSApp(), windows];
        let count: usize = msg_send![windows, count];
        let mut window = nil;
        for index in 0..count {
            let candidate: id = msg_send![windows, objectAtIndex: index];
            let title: id = msg_send![candidate, title];
            let text: *const std::ffi::c_char = msg_send![title, UTF8String];
            if !text.is_null() && std::ffi::CStr::from_ptr(text).to_bytes() == b"Herdr" {
                window = candidate;
                break;
            }
        }
        if window == nil {
            return Err("fixture window not found".into());
        }
        let children: id = msg_send![window.contentView(), subviews];
        let count: usize = msg_send![children, count];
        let mut view = nil;
        for index in 0..count {
            let child: id = msg_send![children, objectAtIndex: index];
            if (*child).class().name() == "GPUIView" {
                view = child;
                break;
            }
        }
        if view == nil {
            return Err("fixture GPUIView missing".into());
        }
        let number: isize = msg_send![window, windowNumber];
        for kind in [1_usize, 2] {
            let event: id = msg_send![class!(NSEvent), mouseEventWithType: kind
                location: NSPoint::new(x, NSView::frame(view).size.height - y)
                modifierFlags: 0_usize timestamp: 0_f64 windowNumber: number
                context: nil eventNumber: 0_isize clickCount: 1_isize pressure: 1_f32];
            if event == nil {
                return Err("cannot create fixture click".into());
            }
            if kind == 1 {
                let _: () = msg_send![view, mouseDown: event];
            } else {
                let _: () = msg_send![view, mouseUp: event];
            }
        }
        Ok(())
    }
}
