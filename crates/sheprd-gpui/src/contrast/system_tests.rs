use super::*;

#[test]
fn a_system_contrast_theme_raises_contrast_and_never_lowers_it() {
    assert_eq!(Contrast::Standard.or(true), Contrast::High);
    assert_eq!(Contrast::High.or(true), Contrast::High);
    assert_eq!(Contrast::Standard.or(false), Contrast::Standard);
    assert_eq!(Contrast::High.or(false), Contrast::High);
    // Whatever the machine running the tests uses, the configured level is
    // a floor.
    assert_eq!(Contrast::High.or_system(), Contrast::High);
}
