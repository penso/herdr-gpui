use super::*;

#[test]
fn dwm_colors_unpack_to_rgb() {
    // Windows' default blue, #0078D4, as DWM stores it.
    assert_eq!(rgb(0xffd4_7800), 0x0078d4);
    assert_eq!(rgb(0x0011_2233), 0x332211);
}

#[test]
fn the_accent_reads_or_is_absent() {
    // Either the user's color or nothing; never a stray alpha byte.
    assert!(accent().is_none_or(|color| color <= 0xff_ffff));
}
