use super::*;
use crate::sidebar::{
    ARROW_RESERVE, HOST_ARROW_WIDTH, ICON_RESERVE, line_height, scaled, unknown_dot,
};
use gpui::{Styled, px};

fn sidebar_font(size: f32) -> FontConfig {
    FontConfig {
        size,
        ..crate::config::Config::default().sidebar
    }
}

/// The sizes marks are drawn at, as the default sidebar font sizes them.
const SIZES: &[f32] = &[
    STATUS_WIDTH,
    STATUS_DOT_UNKNOWN,
    ICON_RESERVE,
    ARROW_RESERVE,
    HOST_ARROW_WIDTH,
];

/// The default `[sidebar] size` keeps every mark as it was drawn, twice the
/// size doubles it, and sizes between land on whole pixels near their share.
#[test]
fn marks_scale_with_the_sidebar_font() {
    let default = crate::config::Config::default().sidebar.size;
    for &size in SIZES {
        assert_eq!(scaled(&sidebar_font(default), size), size);
        assert_eq!(scaled(&sidebar_font(2. * default), size), 2. * size);
        for font_size in [8., 12.5, 13., 17., 48.] {
            let mark = scaled(&sidebar_font(font_size), size);
            assert_eq!(mark, mark.round(), "{size} at {font_size}");
            assert!(
                (mark - size * font_size / default).abs() <= 0.5,
                "{size} at {font_size}: {mark}"
            );
        }
    }
}

/// A dot's slot is as wide and tall as the dot, which grows with the font,
/// and stays centered on the first line; the unknown dot stays as much
/// smaller as it is at the default size.
#[test]
fn status_dots_grow_with_the_sidebar_font() {
    let default = crate::config::Config::default().sidebar.size;
    let indicators = Indicators::new(None, StatusIcons::Dots, false, false, &Theme::default());
    for factor in [1., 2.] {
        let font = sidebar_font(default * factor);
        let dot = indicators.width(&font);
        assert_eq!(dot, STATUS_WIDTH * factor);
        assert_eq!(unknown_dot(dot), STATUS_DOT_UNKNOWN * factor);
        for status in STATUSES {
            let mut slot = status_indicator(status, &font, indicators);
            let style = slot.style();
            assert_eq!(style.size.width, Some(px(dot).into()), "{status:?}");
            assert_eq!(style.size.height, Some(px(dot).into()), "{status:?}");
            assert_eq!(
                style.margin.top,
                Some(px((line_height(&font) - dot) / 2.).into()),
                "{status:?}"
            );
        }
    }
}
