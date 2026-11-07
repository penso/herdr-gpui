//! What the Windows taskbar button shows: an overlay badge while agents need
//! attention, and a progress bar while a window copies files or moves work
//! elsewhere.
//!
//! Every top-level window has its own taskbar button, and windows sharing the
//! AppUserModelID are grouped into one. The grouped button shows the overlay
//! of whichever window last set one, so the badge is applied to every window
//! alike. The taskbar copies an overlay icon, so each is destroyed right after
//! it is set.
//!
//! All of this runs on the UI thread, where GPUI has initialized COM (OLE) as
//! a single-threaded apartment, so the interface is created and used there
//! and kept in a thread-local rather than a global that would need `Send`.

mod badge;
mod created;
#[cfg(test)]
mod tests;

use gpui::{Window, WindowId};
use std::{cell::RefCell, collections::HashMap};
use windows::Win32::{
    Foundation::HWND,
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    UI::Shell::{
        ITaskbarList3, TBPF_INDETERMINATE, TBPF_NOPROGRESS, TBPF_NORMAL, TBPFLAG, TaskbarList,
    },
};
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("could not reach the taskbar")]
    Taskbar(#[source] windows::core::Error),
    #[error("could not draw the taskbar badge")]
    Badge(#[source] windows::core::Error),
    #[error("could not set the taskbar badge")]
    Overlay(#[source] windows::core::Error),
    #[error("could not set the taskbar progress")]
    Progress(#[source] windows::core::Error),
}

/// What one window's button shows, compared before calling into the shell
/// so the poll loop sets it only when it changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Bar {
    #[default]
    None,
    Busy,
    /// Done, out of [`Bar::TOTAL`].
    Value(u64),
}

impl Bar {
    const TOTAL: u64 = 1000;

    pub(crate) fn from_progress(progress: Option<crate::progress::Progress>) -> Self {
        use crate::progress::Progress;
        match progress {
            None => Self::None,
            Some(Progress::Busy) => Self::Busy,
            Some(Progress::Fraction(done) | Progress::Working(done)) => {
                Self::Value((f64::from(done.clamp(0., 1.)) * Self::TOTAL as f64).round() as u64)
            }
        }
    }

    fn state(self) -> TBPFLAG {
        match self {
            Self::None => TBPF_NOPROGRESS,
            Self::Busy => TBPF_INDETERMINATE,
            Self::Value(_) => TBPF_NORMAL,
        }
    }
}

#[derive(Default)]
struct State {
    /// `None` until first needed; `Some(None)` once creating it failed, so a
    /// broken shell is not asked again on every change.
    taskbar: Option<Option<ITaskbarList3>>,
    windows: HashMap<WindowId, isize>,
    overlay: usize,
    bars: HashMap<WindowId, Bar>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

impl State {
    fn taskbar(&mut self) -> Option<&ITaskbarList3> {
        self.taskbar
            .get_or_insert_with(|| {
                create()
                    .inspect_err(|error| tracing::warn!(%error, "taskbar integration is off"))
                    .ok()
            })
            .as_ref()
    }

    /// Sets every window's badge again, and forgets the progress shown, so
    /// the next poll sets it again too.
    fn reapply(&mut self) {
        self.bars.clear();
        if self.overlay == 0 {
            return;
        }
        let windows: Vec<isize> = self.windows.values().copied().collect();
        for hwnd in windows {
            self.apply_overlay(hwnd);
        }
    }

    fn apply_overlay(&mut self, hwnd: isize) {
        let count = self.overlay;
        let Some(taskbar) = self.taskbar() else {
            return;
        };
        if let Err(error) = badge::set(taskbar, HWND(hwnd as *mut _), count) {
            tracing::debug!(%error, "taskbar badge was not set");
        }
    }
}

// The taskbar is reached only through COM, which has no safe wrapper.
#[allow(unsafe_code)]
fn create() -> Result<ITaskbarList3, Error> {
    // SAFETY: COM is initialized on this (the UI) thread by GPUI's Windows
    // platform before any window exists, and the class ID and interface are
    // the documented pair for the taskbar list.
    let taskbar: ITaskbarList3 =
        unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) }
            .map_err(Error::Taskbar)?;
    // SAFETY: `HrInit` must be called once before the other methods.
    unsafe { taskbar.HrInit() }.map_err(Error::Taskbar)?;
    Ok(taskbar)
}

/// Notes a window's native handle the first time it is seen, and gives it the
/// current badge. When Explorer created a button since, for a new window or
/// after Explorer restarted, every window's badge and progress are set again.
/// Cheap enough for the poll loop: otherwise a flag and one map lookup.
pub(crate) fn track(window: &Window) {
    if created::take() {
        STATE.with_borrow_mut(State::reapply);
    }
    let id = window.window_handle().window_id();
    if STATE.with_borrow(|state| state.windows.contains_key(&id)) {
        return;
    }
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = handle.hwnd.get();
    // Watch first, so a button created right after is not missed.
    created::watch(HWND(hwnd as *mut _));
    STATE.with_borrow_mut(|state| {
        state.windows.insert(id, hwnd);
        if state.overlay > 0 {
            state.apply_overlay(hwnd);
        }
    });
}

/// Drops windows that closed, so a reused handle is never badged by mistake.
pub(crate) fn retain(open: &[gpui::AnyWindowHandle]) {
    let is_open = |id: &WindowId| open.iter().any(|window| window.window_id() == *id);
    STATE.with_borrow_mut(|state| {
        state.windows.retain(|id, _| is_open(id));
        state.bars.retain(|id, _| is_open(id));
    });
}

/// Shows `bar` on one window's taskbar button, if it changed. A window not
/// yet tracked is skipped; [`track`] runs first in the same poll.
pub(crate) fn set_progress(window: WindowId, bar: Bar) {
    STATE.with_borrow_mut(|state| {
        if state.bars.get(&window).copied().unwrap_or_default() == bar {
            return;
        }
        let Some(hwnd) = state.windows.get(&window).copied() else {
            return;
        };
        let Some(taskbar) = state.taskbar() else {
            return;
        };
        match apply_progress(taskbar, HWND(hwnd as *mut _), bar) {
            Ok(()) => {
                state.bars.insert(window, bar);
            }
            // Not recorded, so the next poll tries again.
            Err(error) => tracing::debug!(%error, "taskbar progress was not set"),
        }
    });
}

// The taskbar is reached only through COM, which has no safe wrapper.
#[allow(unsafe_code)]
fn apply_progress(taskbar: &ITaskbarList3, hwnd: HWND, bar: Bar) -> Result<(), Error> {
    // SAFETY: `hwnd` is a GPUI window of this thread; one that has since
    // closed only makes the calls fail. The state is set first because
    // setting a value also switches an indeterminate bar to normal.
    unsafe {
        taskbar
            .SetProgressState(hwnd, bar.state())
            .map_err(Error::Progress)?;
        if let Bar::Value(done) = bar {
            taskbar
                .SetProgressValue(hwnd, done, Bar::TOTAL)
                .map_err(Error::Progress)?;
        }
    }
    Ok(())
}

/// Shows `count` on every window's taskbar button, or clears it at zero.
/// Headless tests never reach the shell (see `app_badge::set_native`).
#[cfg(not(test))]
pub(crate) fn set_overlay(count: usize) {
    STATE.with_borrow_mut(|state| {
        state.overlay = count;
        let windows: Vec<isize> = state.windows.values().copied().collect();
        for hwnd in windows {
            state.apply_overlay(hwnd);
        }
    });
}
