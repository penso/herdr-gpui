use super::*;

const LINK: &str = "https://192.168.1.20:7443/pair#t=MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY\
                    &fp=5F:2A:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:C1:9E";

#[test]
fn a_pairing_link_encodes_with_its_finder_patterns() {
    let qr = Qr::encode(LINK).unwrap();
    let width = qr.width();
    // Versions run 21 to 177 modules, four apart.
    assert!(
        (21..=177).contains(&width) && (width - 17).is_multiple_of(4),
        "{width}"
    );
    // The three finder squares' dark corners, and their light separators.
    for (x, y) in [(0, 0), (width - 1, 0), (0, width - 1)] {
        assert!(qr.is_dark(x, y), "({x}, {y})");
    }
    assert!(!qr.is_dark(7, 7));
    assert!(!qr.is_dark(width, 0), "outside the code is light");
    assert!(!qr.is_dark(0, width));
}

#[test]
fn runs_cover_exactly_the_dark_modules() {
    let qr = Qr::encode(LINK).unwrap();
    let runs = qr.runs();
    let mut painted = vec![false; qr.width() * qr.width()];
    for run in runs {
        assert!(run.length > 0);
        assert!(run.column + run.length <= qr.width());
        for column in run.column..run.column + run.length {
            let cell = &mut painted[run.row * qr.width() + column];
            assert!(!*cell, "painted twice");
            *cell = true;
        }
    }
    for y in 0..qr.width() {
        for x in 0..qr.width() {
            assert_eq!(painted[y * qr.width() + x], qr.is_dark(x, y), "({x}, {y})");
        }
    }
    // A finder pattern's top row is one run of seven.
    assert!(runs.contains(&Run {
        row: 0,
        column: 0,
        length: 7
    }));
    assert!(runs.len() < qr.width() * qr.width() / 2);
}

#[test]
fn modules_are_whole_pixels_centred_with_their_quiet_zone() {
    // 25 modules and 8 quiet ones in 200 px: 6 px each, 198 px, 1 px margin.
    let layout = layout(200., 25).unwrap();
    assert_eq!(layout.module, 6.);
    assert_eq!(layout.size, 198.);
    assert_eq!(layout.margin, 1.);
    assert_eq!(layout.origin, 1. + 6. * 4.);
    // The code and its quiet zone always fit.
    for side in [33., 64., 99.5, 180., 240., 1000.] {
        for modules in [21, 25, 33, 57] {
            let Some(layout) = super::layout(side, modules) else {
                assert!(side < (modules + 8) as f32, "{side} {modules}");
                continue;
            };
            assert_eq!(layout.module.fract(), 0.);
            assert!(layout.margin + layout.size <= side);
            assert!(layout.margin >= 0.);
            // Centred to within a pixel and the square's own fraction.
            let right = side - layout.margin - layout.size;
            assert!((right - layout.margin).abs() < 2., "centred");
        }
    }
}

#[test]
fn a_square_too_small_or_a_code_with_no_modules_has_no_layout() {
    assert_eq!(layout(28., 21), None);
    assert_eq!(layout(200., 0), None);
    assert_eq!(layout(f32::NAN, 21), None);
    assert_eq!(layout(-5., 21), None);
    assert!(layout(29., 21).is_some());
}

#[test]
fn a_link_too_long_for_any_code_is_an_error() {
    let long = "x".repeat(5000);
    assert!(matches!(Qr::encode(&long), Err(Error::Qr(_))));
}
