//! The Win32 side of the taskbar identity: each window's shell property store.
//! No safe wrapper exposes `SHGetPropertyStoreForWindow` or `IPropertyStore`,
//! so the calls below are `unsafe`, each scoped to one FFI call.

use std::{ffi::OsStr, mem::ManuallyDrop, num::NonZeroIsize, os::windows::ffi::OsStrExt};

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::{
    Win32::{
        Foundation::{HWND, PROPERTYKEY},
        Storage::EnhancedStorage::{
            PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand,
            PKEY_AppUserModel_RelaunchDisplayNameResource, PKEY_AppUserModel_RelaunchIconResource,
        },
        System::{
            Com::StructuredStorage::{
                PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
            },
            Variant::VT_LPWSTR,
        },
        UI::Shell::{
            PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
            SHStrDupW,
        },
    },
    core::PCWSTR,
};

use crate::taskbar::Relaunch;

/// A marked window's handle, kept as an integer so the global holds no pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Hwnd(NonZeroIsize);

impl Hwnd {
    fn get(self) -> HWND {
        HWND(self.0.get() as *mut core::ffi::c_void)
    }
}

// The ID goes last: the shell ignores a window's relaunch properties unless
// the window has an explicit AppUserModelID, and setting the ID regroups the
// window, so the relaunch properties must already exist when it does.
const KEYS: [(PROPERTYKEY, &str); 4] = [
    (
        PKEY_AppUserModel_RelaunchCommand,
        "System.AppUserModel.RelaunchCommand", // i18n: data
    ),
    (
        PKEY_AppUserModel_RelaunchDisplayNameResource,
        "System.AppUserModel.RelaunchDisplayNameResource", // i18n: data
    ),
    (
        PKEY_AppUserModel_RelaunchIconResource,
        "System.AppUserModel.RelaunchIconResource", // i18n: data
    ),
    (PKEY_AppUserModel_ID, "System.AppUserModel.ID"), // i18n: data
];

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("the window has no native handle: {0}")]
    Handle(raw_window_handle::HandleError),
    #[error("the window's native handle is not a Win32 window")]
    NotWin32,
    #[error("unable to set {property} on the window")]
    Property {
        property: &'static str,
        #[source]
        source: windows::core::Error,
    },
    #[error("unable to open the window's property store")]
    Store(#[source] windows::core::Error),
}

/// Sets `relaunch` on `window` and returns its handle, so the properties can
/// be cleared before it closes. A failure is logged and leaves the window
/// with the default identity, so a pin opens `herdr-gpui.exe` as before.
pub(super) fn apply(window: &Window, relaunch: &Relaunch) -> Option<Hwnd> {
    let hwnd = match hwnd(window) {
        Ok(hwnd) => hwnd,
        Err(error) => {
            warn(
                &error,
                "Unable to find the window's Win32 handle; pinning it pins herdr-gpui.exe", // i18n: data
            );
            return None;
        }
    };
    if let Err(error) = set(hwnd, relaunch) {
        warn(
            &error,
            "Unable to set the window's taskbar relaunch properties; pinning it pins herdr-gpui.exe", // i18n: data
        );
    }
    Some(hwnd)
}

/// Removes the properties from a closing window. The shell does not free them
/// itself: "A window's properties must be removed before the window is
/// closed. ... A property is removed by setting it to the PROPVARIANT type
/// VT_EMPTY" (`SHGetPropertyStoreForWindow`, Microsoft Learn).
#[allow(unsafe_code)]
pub(super) fn clear(hwnd: Hwnd) {
    let store = match open(hwnd) {
        Ok(store) => store,
        Err(error) => {
            warn(
                &error,
                "Unable to clear the closing window's taskbar properties", // i18n: data
            );
            return;
        }
    };
    for (key, property) in KEYS.iter().rev() {
        let empty = PROPVARIANT::default();
        // SAFETY: `key` points to a constant and `empty` to a live VT_EMPTY
        // value; both are only read during the call and outlive it.
        if let Err(source) = unsafe { store.SetValue(key, &empty) } {
            let error = Error::Property { property, source };
            warn(
                &error,
                "Unable to clear the closing window's taskbar properties", // i18n: data
            );
        }
    }
}

fn hwnd(window: &Window) -> Result<Hwnd, Error> {
    let handle = HasWindowHandle::window_handle(window).map_err(Error::Handle)?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Ok(Hwnd(handle.hwnd)),
        _ => Err(Error::NotWin32),
    }
}

#[allow(unsafe_code)]
fn set(hwnd: Hwnd, relaunch: &Relaunch) -> Result<(), Error> {
    let store = open(hwnd)?;
    let values = [
        relaunch.command.as_os_str(),
        OsStr::new(relaunch.display_name),
        relaunch.icon.as_os_str(),
        OsStr::new(relaunch.id),
    ];
    for ((key, property), value) in KEYS.iter().zip(values) {
        let failed = |source| Error::Property { property, source };
        let value = lpwstr(value).map_err(failed)?;
        // SAFETY: `key` points to a constant and `value` to a live VT_LPWSTR
        // PROPVARIANT; both are only read during the call, which stores its
        // own copy, and are dropped after it returns.
        unsafe { store.SetValue(key, &value) }.map_err(failed)?;
    }
    Ok(())
}

#[allow(unsafe_code)]
fn open(hwnd: Hwnd) -> Result<IPropertyStore, Error> {
    // SAFETY: `hwnd` is a top-level window GPUI created on this UI thread and
    // has not destroyed: `apply` runs while the window is built, and `clear`
    // runs from the window-closed observer, which GPUI calls before it
    // destroys the window. The returned interface is owned by `IPropertyStore`.
    unsafe { SHGetPropertyStoreForWindow(hwnd.get()) }.map_err(Error::Store)
}

/// A VT_LPWSTR value, as propvarutil.h's `InitPropVariantFromString` builds
/// one: the shell's AppUserModel properties are strings of that type.
#[allow(unsafe_code)]
fn lpwstr(value: &OsStr) -> windows::core::Result<PROPVARIANT> {
    let wide: Vec<u16> = value.encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call, which only reads it.
    let copy = unsafe { SHStrDupW(PCWSTR(wide.as_ptr())) }?;
    // The copy is CoTaskMemAlloc memory the PROPVARIANT now owns: its Drop runs
    // PropVariantClear, which frees a VT_LPWSTR with CoTaskMemFree.
    Ok(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_LPWSTR,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 { pwszVal: copy },
            }),
        },
    })
}

fn warn(error: &Error, message: &str) {
    let source = std::error::Error::source(error).map(ToString::to_string);
    tracing::warn!(%error, source = source.as_deref(), "{message}");
}
