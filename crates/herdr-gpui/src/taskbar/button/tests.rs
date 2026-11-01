use super::badge::{compose, description, label};

#[test]
fn label_fits_two_characters() {
    assert_eq!(label(1), "1");
    assert_eq!(label(9), "9");
    assert_eq!(label(10), "9+");
    assert_eq!(label(usize::MAX), "9+");
}

#[test]
fn description_counts_agents() {
    assert_eq!(description(1), "\u{2068}1\u{2069} agent needs attention");
    assert_eq!(description(3), "\u{2068}3\u{2069} agents need attention");
}

#[test]
fn description_is_a_message_under_en_xa() {
    use herdr_i18n::{Locale, Pseudo};
    crate::i18n::show(&Pseudo::Accented.locale());
    let shown = [description(1), description(12)];
    crate::i18n::show(&Locale::FALLBACK);
    for line in shown {
        assert!(herdr_i18n::pseudo::is_marked(&line), "{line}");
    }
}

#[test]
fn disc_is_opaque_inside_clear_outside_and_text_is_white() {
    let size = 16;
    let mut text = vec![0; size * size];
    let centre = 8 * size + 8;
    text[centre] = 255;
    let pixels = compose(size, &text);
    assert_eq!(pixels.len(), size * size);
    // Corners lie outside the disc.
    assert_eq!(pixels[0][3], 0);
    assert_eq!(pixels[size * size - 1][3], 0);
    // Inside: opaque red, with white where the text is.
    let inside = pixels[centre + 2];
    assert_eq!(inside, [0x4D, 0x48, 0xE5, 255]);
    assert_eq!(pixels[centre], [255, 255, 255, 255]);
}

#[test]
fn missing_coverage_is_treated_as_no_text() {
    let pixels = compose(4, &[]);
    assert!(
        pixels
            .iter()
            .all(|pixel| pixel[0] == 0x4D || pixel[3] < 255)
    );
}

#[test]
fn progress_maps_to_the_taskbar_bar() {
    use super::Bar;
    use crate::progress::Progress;
    assert_eq!(Bar::from_progress(None), Bar::None);
    assert_eq!(Bar::from_progress(Some(Progress::Busy)), Bar::Busy);
    assert_eq!(
        Bar::from_progress(Some(Progress::Fraction(0.25))),
        Bar::Value(250)
    );
    assert_eq!(
        Bar::from_progress(Some(Progress::Working(2.))),
        Bar::Value(1000)
    );
    assert_eq!(
        Bar::from_progress(Some(Progress::Fraction(-1.))),
        Bar::Value(0)
    );
}
