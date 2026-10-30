use super::*;
use gpui::{Anchor, StyleRefinement, TextAlign, px};
use herdr_i18n::{Locale, Pseudo};

fn rtl() {
    crate::i18n::show(&Pseudo::Bidi.locale());
}

/// The style a refinement builds, as the element would carry it.
fn style(build: impl FnOnce(StyleRefinement) -> StyleRefinement) -> StyleRefinement {
    build(StyleRefinement::default())
}

#[test]
fn english_lays_out_left_to_right() {
    crate::i18n::show(&Locale::FALLBACK);
    assert_eq!(current(), Direction::Ltr);
    assert_eq!(
        style(|s| s.ps(px(4.)).pe(px(9.))),
        style(|s| s.pl(px(4.)).pr(px(9.)))
    );
    assert_eq!(style(|s| s.row()), style(|s| s.flex().flex_row()));
    assert_eq!(anchor(Anchor::TopLeft), Anchor::TopLeft);
    assert_eq!(toward_end(-3.), -3.);
    assert_eq!(icon("icons/chevron-right.svg"), "icons/chevron-right.svg");
}

#[test]
fn the_bidi_pseudo_locale_mirrors_every_side() {
    rtl();
    assert_eq!(current(), Direction::Rtl);
    let pairs: [(StyleRefinement, StyleRefinement); 9] = [
        (style(|s| s.ps(px(4.))), style(|s| s.pr(px(4.)))),
        (style(|s| s.pe(px(4.))), style(|s| s.pl(px(4.)))),
        (style(|s| s.ms(px(4.))), style(|s| s.mr(px(4.)))),
        (style(|s| s.me(px(4.))), style(|s| s.ml(px(4.)))),
        (style(|s| s.inset_s(px(4.))), style(|s| s.right(px(4.)))),
        (style(|s| s.inset_e(px(4.))), style(|s| s.left(px(4.)))),
        (style(|s| s.border_s(px(1.))), style(|s| s.border_r(px(1.)))),
        (style(|s| s.border_e(px(1.))), style(|s| s.border_l(px(1.)))),
        (
            style(|s| s.rounded_e(px(3.))),
            style(|s| s.rounded_l(px(3.))),
        ),
    ];
    for (logical, physical) in pairs {
        assert_eq!(logical, physical);
    }
    assert_eq!(style(|s| s.row()), style(|s| s.flex().flex_row_reverse()));
    assert_eq!(style(|s| s.text_start()), style(|s| s.text_right()));
    assert_eq!(style(|s| s.text_end()), style(|s| s.text_left()));
    assert_eq!(text_start(), TextAlign::Right);
}

#[test]
fn anchors_mirror_across_the_vertical_axis_only() {
    rtl();
    for (written, mirrored) in [
        (Anchor::TopLeft, Anchor::TopRight),
        (Anchor::BottomLeft, Anchor::BottomRight),
        (Anchor::LeftCenter, Anchor::RightCenter),
        (Anchor::TopCenter, Anchor::TopCenter),
        (Anchor::BottomCenter, Anchor::BottomCenter),
    ] {
        assert_eq!(anchor(written), mirrored);
        assert_eq!(anchor(mirrored), written);
    }
    assert_eq!(toward_end(-3.), 3.);
}

#[test]
fn directional_icons_swap_with_an_embedded_mirror() {
    rtl();
    for (icon_path, mirror) in DIRECTIONAL_ICONS {
        assert_eq!(icon(icon_path), *mirror);
        assert_eq!(icon(mirror), *icon_path);
        for path in [icon_path, mirror] {
            let asset = gpui::AssetSource::load(&crate::icons::Icons, path);
            assert!(matches!(asset, Ok(Some(_))), "{path} has no embedded asset");
        }
    }
    // Icons that point nowhere in particular, or up and down, stay.
    for still in [
        "icons/chevron-down.svg",
        "icons/close.svg",
        "icons/arrow-up.svg",
    ] {
        assert_eq!(icon(still), still);
    }
}

#[test]
fn text_arrows_swap_with_their_mirror() {
    rtl();
    for (glyph_text, mirror) in DIRECTIONAL_GLYPHS {
        assert_eq!(glyph(glyph_text), *mirror);
        assert_eq!(glyph(mirror), *glyph_text);
    }
    assert_eq!(glyph("\u{25be}"), "\u{25be}");
    crate::i18n::show(&Locale::FALLBACK);
    assert_eq!(glyph("\u{25b8}"), "\u{25b8}");
}
