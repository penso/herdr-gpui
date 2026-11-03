//! DWM window attributes GPUI does not set from the app's own state.
use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::OnceLock;
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE,
    DwmSetWindowAttribute,
};

pub(crate) use windows_sys::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWMSBT_AUTO, DWMSBT_TRANSIENTWINDOW,
};

/// Windows 11 22H2, the first build with `DWMWA_SYSTEMBACKDROP_TYPE`.
const SYSTEM_BACKDROP_BUILD: u32 = 22_621;

/// Darkens or lightens the system caption of `window`.
pub(crate) fn dark_caption(window: &Window, dark: bool) {
    set(window, DWMWA_USE_IMMERSIVE_DARK_MODE, i32::from(dark));
}

/// Whether DWM paints system backdrops (Mica, acrylic) on this Windows.
pub(crate) fn system_backdrops() -> bool {
    static BUILD: OnceLock<u32> = OnceLock::new();
    *BUILD.get_or_init(build) >= SYSTEM_BACKDROP_BUILD
}

/// Sets the material DWM paints behind `window`'s transparent pixels, and
/// its caption. `DWMSBT_AUTO` gives the choice back to DWM.
pub(crate) fn system_backdrop(window: &Window, material: DWM_SYSTEMBACKDROP_TYPE) {
    set(window, DWMWA_SYSTEMBACKDROP_TYPE, material);
}

/// The running Windows build. `GetVersionExW` reports what the executable's
/// manifest claims to support rather than the real build; `RtlGetVersion`
/// reports the real one, which is also what GPUI checks.
fn build() -> u32 {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a properly sized, writable OSVERSIONINFOW whose size
    // field is set, as RtlGetVersion requires; it outlives the call.
    #[allow(unsafe_code)]
    let status = unsafe { RtlGetVersion(&mut info) };
    if status < 0 { 0 } else { info.dwBuildNumber }
}

/// Sets a 32-bit attribute: a `BOOL` or a backdrop type. A failure leaves the
/// window as GPUI drew it, which is still usable, so it is logged rather than
/// surfaced.
fn set(window: &Window, attribute: DWMWINDOWATTRIBUTE, value: i32) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    // Safe Rust has no DWM binding; windows-sys is the plain FFI declaration.
    // SAFETY: the handle comes from a live GPUI window borrowed for this
    // call, so the HWND is valid; `value` outlives the call and its size is
    // passed with it. DwmSetWindowAttribute only reads the buffer.
    #[allow(unsafe_code)]
    let result = unsafe {
        DwmSetWindowAttribute(
            handle.hwnd.get() as _,
            attribute as u32,
            std::ptr::from_ref(&value).cast(),
            size_of::<i32>() as u32,
        )
    };
    if result < 0 {
        tracing::debug!(attribute, result, "DwmSetWindowAttribute failed");
    }
}
