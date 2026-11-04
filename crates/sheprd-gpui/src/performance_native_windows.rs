//! Win32 adapter for the performance fixture: hover and wheel messages sent to
//! the fixture's own window, never posted to the desktop or another process.
#![allow(unsafe_code)]
use anyhow::{Context as _, Result, bail};
use std::{marker::PhantomData, rc::Rc};
use windows_sys::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::ClientToScreen,
    UI::WindowsAndMessaging::{IsWindow, SendMessageW, WHEEL_DELTA, WM_MOUSEMOVE, WM_MOUSEWHEEL},
};

// raw-window-handle does not implement Error without its optional std feature.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct NativeHandleError(raw_window_handle::HandleError);

/// The fixture window, valid on the UI thread that created it.
#[derive(Debug)]
pub(crate) struct Target {
    hwnd: HWND,
    scale: f64,
    _main_thread: PhantomData<Rc<()>>,
}

impl Target {
    pub(crate) fn acquire(window: &gpui::Window) -> Result<Self> {
        let handle = raw_window_handle::HasWindowHandle::window_handle(window)
            .map_err(NativeHandleError)
            .context("acquiring native fixture window handle")?;
        let raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw() else {
            bail!("fixture is not a Win32 window");
        };
        Ok(Self {
            hwnd: handle.hwnd.get() as HWND,
            scale: f64::from(window.scale_factor()),
            _main_thread: PhantomData,
        })
    }

    /// Moves the pointer to logical `(100, y)` in the window, then, with
    /// `scroll`, turns the wheel one notch in its direction (negative scrolls
    /// the content up, as AppKit's sign does).
    pub(crate) fn dispatch(&self, y: f64, scroll: Option<i32>) -> Result<()> {
        // Device pixels, as Win32 messages carry them; both fit in 16 bits for
        // the fixture's 1640x1100 window at any desktop scale.
        let client = POINT {
            x: (100. * self.scale).round() as i32,
            y: (y * self.scale).round() as i32,
        };
        // SAFETY: `hwnd` came from GPUI for a window this thread owns, and
        // IsWindow only reads it. SendMessageW runs that window's procedure
        // synchronously on this thread; the caller holds no GPUI borrow, so
        // the procedure may reenter GPUI. Both messages carry values, never
        // pointers.
        unsafe {
            if IsWindow(self.hwnd) == 0 {
                bail!("fixture window was destroyed");
            }
            SendMessageW(self.hwnd, WM_MOUSEMOVE, 0, pack(client));
            if let Some(delta) = scroll {
                // WM_MOUSEWHEEL carries screen coordinates.
                let mut screen = client;
                if ClientToScreen(self.hwnd, &mut screen) == 0 {
                    bail!("cannot map the fixture point to the screen");
                }
                let notch = WHEEL_DELTA as i32 * delta.signum();
                let wparam = ((notch as u32 & 0xffff) << 16) as usize;
                SendMessageW(self.hwnd, WM_MOUSEWHEEL, wparam, pack(screen));
            }
        }
        Ok(())
    }
}

/// MAKELPARAM: signed 16-bit x in the low word, y in the high word.
fn pack(point: POINT) -> isize {
    ((point.x as u32 & 0xffff) | ((point.y as u32 & 0xffff) << 16)) as i32 as isize
}
