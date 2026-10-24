use super::*;
use std::sync::mpsc::channel;

/// A 2x1 32-bit DIB with a plain info header and BI_BITFIELDS masks.
fn dib() -> Vec<u8> {
    let mut dib = Vec::new();
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&2i32.to_le_bytes());
    dib.extend_from_slice(&1i32.to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&32u16.to_le_bytes());
    dib.extend_from_slice(&3u32.to_le_bytes());
    dib.extend_from_slice(&8u32.to_le_bytes());
    dib.extend_from_slice(&[0; 16]);
    for mask in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff] {
        dib.extend_from_slice(&mask.to_le_bytes());
    }
    dib.extend_from_slice(&[0, 0, 255, 0, 0, 255, 0, 0]);
    dib
}

#[test]
fn a_dib_becomes_a_bmp_the_image_crate_decodes() {
    let dib = dib();
    let bmp = bmp_from_dib(&dib).unwrap();
    assert_eq!(&bmp[..2], b"BM");
    assert_eq!(
        u32::from_le_bytes(bmp[2..6].try_into().unwrap()) as usize,
        bmp.len()
    );
    // Header, info header, and the three masks precede the pixels.
    assert_eq!(
        u32::from_le_bytes(bmp[10..14].try_into().unwrap()),
        14 + 40 + 12
    );
    assert_eq!(&bmp[14..], dib.as_slice());
    let image = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).unwrap();
    assert_eq!((image.width(), image.height()), (2, 1));
}

#[test]
fn malformed_dibs_are_skipped_not_misread() {
    assert!(bmp_from_dib(&[]).is_none());
    assert!(bmp_from_dib(&dib()[..39]).is_none());
    // An info header shorter than the smallest defined one.
    let mut short = dib();
    short[..4].copy_from_slice(&12u32.to_le_bytes());
    assert!(bmp_from_dib(&short).is_none());
    // A palette that would put the pixels past the data.
    let mut palette = dib();
    palette[14..16].copy_from_slice(&8u16.to_le_bytes());
    palette[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(bmp_from_dib(&palette).is_none());
}

#[test]
fn a_reader_that_never_returns_times_out_and_blocks_the_next() {
    static BUSY: AtomicBool = AtomicBool::new(false);
    let (release, wait) = channel::<()>();
    let (done, finished) = channel();
    let start = Instant::now();
    let result: Result<()> = bounded(&BUSY, Duration::from_millis(50), move |_| {
        let _ = wait.recv();
        let _ = done.send(());
        Ok(())
    });
    assert!(matches!(
        result,
        Err(crate::Error::Clipboard(Error::Timeout))
    ));
    assert!(start.elapsed() < Duration::from_secs(5));
    // The stuck reader still holds its slot: no second thread starts.
    let refused: Result<()> = bounded(&BUSY, Duration::from_secs(5), |_| {
        panic!("a second reader must not start")
    });
    assert!(matches!(
        refused,
        Err(crate::Error::Clipboard(Error::Timeout))
    ));
    release.send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(5)).unwrap();
    // Once it returns, the slot is free again.
    for _ in 0..500 {
        if !BUSY.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let value = bounded(&BUSY, Duration::from_secs(5), |_| Ok(7)).unwrap();
    assert_eq!(value, 7);
}

#[test]
fn reader_errors_and_deadline_reach_the_caller() {
    static BUSY: AtomicBool = AtomicBool::new(false);
    let result: Result<()> = bounded(&BUSY, Duration::from_secs(5), |deadline| {
        assert!(deadline > Instant::now());
        Err(Error::Changed.into())
    });
    assert!(matches!(
        result,
        Err(crate::Error::Clipboard(Error::Changed))
    ));
}

/// Reads the real clipboard, so it is opt-in: copy an image first (for example
/// with Snipping Tool), then run `cargo test -p herdr-gpui --bins
/// native_clipboard_image -- --ignored --nocapture`.
#[test]
#[ignore = "reads the desktop clipboard; copy an image first"]
fn native_clipboard_image_is_read_off_the_ui_thread() {
    // `None` means the clipboard holds no image.
    let item = read(true).unwrap().unwrap();
    let Some(gpui::ClipboardEntry::Image(image)) = item.entries().first() else {
        panic!("not an image: {:?}", item.entries());
    };
    let format = match image.format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Jpeg => image::ImageFormat::Jpeg,
        ImageFormat::Gif => image::ImageFormat::Gif,
        ImageFormat::Bmp => image::ImageFormat::Bmp,
        ImageFormat::Tiff => image::ImageFormat::Tiff,
        other => panic!("unexpected format {other:?}"),
    };
    let decoded = image::load_from_memory_with_format(&image.bytes, format).unwrap();
    eprintln!(
        "clipboard image: {:?}, {} bytes, {}x{}",
        image.format,
        image.bytes.len(),
        decoded.width(),
        decoded.height()
    );
}
