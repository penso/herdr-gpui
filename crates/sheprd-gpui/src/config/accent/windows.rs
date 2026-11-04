//! The Windows accent color, as Settings > Personalization > Colors sets it.
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

/// Where DWM keeps the accent it draws accented title bars and borders with.
// i18n: data
const KEY: &str = r"Software\Microsoft\Windows\DWM";
// i18n: data
const VALUE: &str = "AccentColor";

/// The accent as packed RGB, or `None` when the value is missing, as it is
/// until the user first picks a color.
pub(super) fn accent() -> Option<u32> {
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, value) = (wide(KEY), wide(VALUE));
    let mut data = 0_u32;
    let mut size = size_of::<u32>() as u32;
    // Safe Rust has no registry API in std; windows-sys is the plain FFI.
    // SAFETY: both names are NUL-terminated UTF-16 buffers alive for the
    // call; `data` is a writable u32 and `size` holds its size, which
    // RRF_RT_REG_DWORD guarantees the value fits.
    #[allow(unsafe_code)]
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            std::ptr::from_mut(&mut data).cast(),
            &mut size,
        )
    };
    (status == 0).then(|| rgb(data))
}

/// DWM stores the color as 0xAABBGGRR; the theme packs 0xRRGGBB.
fn rgb(abgr: u32) -> u32 {
    let [red, green, blue, _] = abgr.to_le_bytes();
    u32::from_be_bytes([0, red, green, blue])
}

#[cfg(test)]
mod tests;
