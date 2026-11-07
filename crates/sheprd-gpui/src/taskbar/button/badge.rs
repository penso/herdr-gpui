//! Draws the attention count as a small-icon-sized badge: a red disc with
//! the count in white, as on the macOS Dock.
//!
//! GDI rasterizes the digits, grayscale-antialiased, as white on black into a
//! bitmap of our own; their coverage then becomes alpha over a disc composed
//! here, so the icon has smooth edges on any taskbar color.
//!
//! GDI and the shell's COM interfaces have no safe wrapper, so the calls are
//! `unsafe`. Each GDI object is owned by a guard that deletes it, and a device
//! context's guard selects its objects back out first, so nothing leaks or is
//! deleted while still in use.
#![allow(unsafe_code)]

use super::Error;
use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, RECT},
        Graphics::Gdi::{
            ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS,
            CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET,
            DIB_RGB_COLORS, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC,
            DeleteObject, DrawTextW, FW_BOLD, GdiFlush, HBITMAP, HDC, HFONT, HGDIOBJ,
            OUT_TT_PRECIS, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
        },
        UI::{
            Shell::ITaskbarList3,
            WindowsAndMessaging::{
                CreateIconIndirect, DestroyIcon, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON,
            },
        },
    },
    core::{HSTRING, w},
};

/// The badge's red, as straight (not premultiplied) RGB.
const FILL: [u8; 3] = [0xE5, 0x48, 0x4D];
/// Icons this size or more are unusual; anything larger is clamped so a
/// strange metric cannot allocate without bound.
const MAX_SIZE: i32 = 64;

/// What the badge reads: the count, or "9+" once it no longer fits.
pub(super) fn label(count: usize) -> String {
    if count > 9 {
        "9+".into()
    } else {
        count.to_string()
    }
}

/// What a screen reader says about the taskbar button.
pub(super) fn description(count: usize) -> String {
    crate::i18n::fl!("taskbar-attention", count = count)
}

/// Straight-alpha BGRA pixels, top row first: a disc filling the square,
/// with the text's coverage (0–255 per pixel, the same layout) in white.
pub(super) fn compose(size: usize, text: &[u8]) -> Vec<[u8; 4]> {
    let radius = size as f32 / 2.;
    (0..size * size)
        .map(|at| {
            let (x, y) = ((at % size) as f32 + 0.5, (at / size) as f32 + 0.5);
            let distance = ((x - radius).powi(2) + (y - radius).powi(2)).sqrt();
            // One pixel of falloff at the rim.
            let disc = (radius - distance + 0.5).clamp(0., 1.);
            let ink = f32::from(text.get(at).copied().unwrap_or(0)) / 255. * disc;
            let channel = |fill: u8| (f32::from(fill) * (1. - ink) + 255. * ink).round() as u8;
            [
                channel(FILL[2]),
                channel(FILL[1]),
                channel(FILL[0]),
                (disc * 255.).round() as u8,
            ]
        })
        .collect()
}

/// Sets the overlay of one window's button: a badge, or none at zero.
pub(super) fn set(taskbar: &ITaskbarList3, hwnd: HWND, count: usize) -> Result<(), Error> {
    if count == 0 {
        // SAFETY: a null icon removes the overlay; `hwnd` is a GPUI window
        // of this thread. A window that has since closed only fails.
        return unsafe { taskbar.SetOverlayIcon(hwnd, HICON::default(), None) }
            .map_err(Error::Overlay);
    }
    let icon = Icon::draw(&label(count))?;
    let description = HSTRING::from(description(count));
    // SAFETY: as above, and the taskbar copies the icon before returning,
    // so `icon` may be destroyed when it drops.
    unsafe { taskbar.SetOverlayIcon(hwnd, icon.0, &description) }.map_err(Error::Overlay)
}

/// An icon this module created, destroyed when dropped.
struct Icon(HICON);

impl Drop for Icon {
    fn drop(&mut self) {
        // SAFETY: the icon came from `CreateIconIndirect` and is not shared.
        let _ = unsafe { DestroyIcon(self.0) };
    }
}

/// A GDI object this module created, deleted when dropped. Declared before
/// the device context that selects it, it outlives that context.
struct Object(HGDIOBJ);

impl Drop for Object {
    fn drop(&mut self) {
        // SAFETY: the object came from a GDI create call and is not selected
        // into a live device context by the time it drops (see `Context`).
        let _ = unsafe { DeleteObject(self.0) };
    }
}

/// A memory device context, deleted when dropped. Whatever was selected into
/// it is selected back out first, so the objects it held can be deleted.
struct Context {
    dc: HDC,
    restore: Vec<HGDIOBJ>,
}

impl Context {
    fn select(&mut self, object: HGDIOBJ) {
        // SAFETY: `dc` is live and `object` is a bitmap or font of ours.
        let previous = unsafe { SelectObject(self.dc, object) };
        self.restore.push(previous);
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        for previous in self.restore.drain(..).rev() {
            // SAFETY: putting back what `select` replaced.
            unsafe { SelectObject(self.dc, previous) };
        }
        // SAFETY: the context came from `CreateCompatibleDC`.
        let _ = unsafe { DeleteDC(self.dc) };
    }
}

impl Icon {
    fn draw(label: &str) -> Result<Self, Error> {
        // SAFETY: reads a system metric; no pointers.
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, MAX_SIZE);
        let pixels = size as usize * size as usize;
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                // Negative: rows run top to bottom, as `compose` writes them.
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        // SAFETY: `info` describes a 32-bit top-down DIB of `size` squared
        // pixels; on success `bits` points at that many zeroed `u32`s, owned
        // by the bitmap and valid until `color` drops.
        let color: HBITMAP =
            unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }
                .map_err(Error::Badge)?;
        let color = Object(color.into());
        // An all-zero mask: the color bitmap's alpha alone decides coverage.
        // SAFETY: a monochrome bitmap GDI fills from no initial bits.
        let mask: HBITMAP = unsafe { CreateBitmap(size, size, 1, 1, None) };
        let mask = Object(mask.into());
        // SAFETY: plain font creation from constants and a static face name.
        let font: HFONT = unsafe {
            CreateFontW(
                -(size * if label.len() > 1 { 9 } else { 12 } / 16),
                0,
                0,
                0,
                FW_BOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_TT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                0,
                w!("Segoe UI"), // i18n: data
            )
        };
        let font = Object(font.into());
        {
            // SAFETY: a memory context compatible with the screen.
            let mut context = Context {
                dc: unsafe { CreateCompatibleDC(None) },
                restore: Vec::new(),
            };
            context.select(color.0);
            context.select(font.0);
            let mut text: Vec<u16> = label.encode_utf16().collect();
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: size,
                bottom: size,
            };
            // SAFETY: draws into the selected DIB; `text` and `rect` live
            // across the call. GdiFlush makes the pixels readable.
            unsafe {
                SetBkMode(context.dc, TRANSPARENT);
                SetTextColor(context.dc, COLORREF(0x00FF_FFFF));
                DrawTextW(
                    context.dc,
                    &mut text,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );
                let _ = GdiFlush();
            }
        }
        // SAFETY: `bits` holds `pixels` 4-byte pixels (see above), nothing
        // else writes them now that drawing is flushed, and `color` keeps
        // them alive for this borrow.
        let canvas = unsafe { std::slice::from_raw_parts_mut(bits.cast::<[u8; 4]>(), pixels) };
        // White on black: any one channel is the glyphs' coverage.
        let coverage: Vec<u8> = canvas.iter().map(|pixel| pixel[1]).collect();
        canvas.copy_from_slice(&compose(size as usize, &coverage));
        let info = ICONINFO {
            fIcon: true.into(),
            hbmMask: HBITMAP(mask.0.0),
            hbmColor: HBITMAP(color.0.0),
            ..Default::default()
        };
        // SAFETY: both bitmaps are live and selected into no context; the
        // icon copies them, so they are deleted when they drop below.
        let icon = unsafe { CreateIconIndirect(&info) }.map_err(Error::Badge)?;
        Ok(Self(icon))
    }
}
