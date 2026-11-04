//! Windows contrast themes (Settings > Accessibility > Contrast themes).
use windows_sys::Win32::UI::{
    Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
    WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW},
};

/// Whether a contrast theme is on.
pub(super) fn high_contrast() -> bool {
    let mut info = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    // Safe Rust has no binding for system parameters; windows-sys is FFI.
    // SAFETY: `info` is a writable HIGHCONTRASTW with its size set, and the
    // size passed matches it; SPI_GETHIGHCONTRAST only writes into it.
    #[allow(unsafe_code)]
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            info.cbSize,
            std::ptr::from_mut(&mut info).cast(),
            0,
        )
    };
    read != 0 && info.dwFlags & HCF_HIGHCONTRASTON != 0
}
