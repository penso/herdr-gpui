use super::*;

#[test]
fn large_image_output_arrives_before_the_deadline() -> crate::Result<()> {
    // The size of a TIFF that AppKit published beside no PNG. A socket pair
    // holds a few kilobytes, so sleeping between reads took over 15 seconds.
    const SIZE: usize = 26 * 1024 * 1024;
    let started = Instant::now();
    let item = acquire(
        &mut shell(&format!(r"printf '\007'; head -c {SIZE} /dev/zero")),
        started + TIMEOUT,
        OUTPUT_LIMIT,
    )?;
    assert!(matches!(item.as_ref().map(ClipboardItem::entries),
        Some([ClipboardEntry::Image(image)])
            if image.format == ImageFormat::Tiff && image.bytes.len() == SIZE));
    assert!(started.elapsed() < TIMEOUT);
    Ok(())
}
