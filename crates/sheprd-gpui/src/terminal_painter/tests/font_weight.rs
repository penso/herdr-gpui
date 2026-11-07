use super::*;
use crate::config::{Config, FontConfig, bold_font_weight};

/// The terminal face as the config builds it, so these checks follow a
/// `[terminal] weight` setting all the way to the painter.
fn terminal_font(weight: FontWeight) -> Font {
    FontConfig {
        weight,
        ..Config::default().terminal
    }
    .font()
}

const REGULAR: usize = 0;

fn bold() -> usize {
    glyphs::style(BOLD)
}

#[test]
fn regular_and_bold_cells_shape_relative_to_the_configured_weight() {
    let font = terminal_font(FontWeight::MEDIUM);
    assert_eq!(font.weight, FontWeight::MEDIUM);
    assert_eq!(
        glyphs::styled_font(&font, REGULAR).weight,
        FontWeight::MEDIUM
    );
    assert_eq!(
        glyphs::styled_font(&font, bold()).weight,
        bold_font_weight(FontWeight::MEDIUM)
    );
    // Italic keeps the weight; bold italic takes both.
    let bold_italic = glyphs::styled_font(&font, glyphs::style(BOLD | ITALIC));
    assert_eq!(
        glyphs::styled_font(&font, glyphs::style(ITALIC)).weight,
        FontWeight::MEDIUM
    );
    assert_eq!(bold_italic.weight, FontWeight::EXTRA_BOLD);
    assert_eq!(bold_italic.style, FontStyle::Italic);

    // The default config draws as it always has: regular, and bold at bold.
    let font = Config::default().terminal.font();
    assert_eq!(
        glyphs::styled_font(&font, REGULAR).weight,
        FontWeight::NORMAL
    );
    assert_eq!(glyphs::styled_font(&font, bold()).weight, FontWeight::BOLD);
}

#[gpui::test]
fn a_weight_only_change_discards_cached_glyphs(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    let painter = std::rc::Rc::new(std::cell::RefCell::new(TerminalPainter::default()));
    let frame = FrameData {
        width: 2,
        height: 1,
        cells: vec![
            cell("x"),
            CellData {
                modifier: BOLD,
                ..cell("x")
            },
        ],
        cursor: None,
        hyperlinks: vec![],
        graphics: vec![],
    };
    let mut draw = |font: Font| {
        let painter = painter.clone();
        let frame = frame.clone();
        cx.draw(Point::default(), size(px(800.), px(600.)), |_, _| {
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    let cell_width = painter.borrow_mut().cell_width(&font, window, cx);
                    painter.borrow_mut().paint_frame(
                        &frame,
                        bounds.origin,
                        None,
                        Ground::Opaque,
                        cell_width,
                        &font,
                        &[],
                        &[],
                        None,
                        None,
                        window,
                        cx,
                    );
                },
            )
            .size_full()
        });
    };
    let cached_styles = |painter: &TerminalPainter| {
        let mut styles: Vec<usize> = painter.glyphs.iter().map(|(style, _, _)| style).collect();
        styles.sort_unstable();
        styles
    };

    let medium = terminal_font(FontWeight::MEDIUM);
    draw(medium.clone());
    assert_eq!(painter.borrow().config.as_ref(), Some(&medium));
    assert_eq!(cached_styles(&painter.borrow()), [REGULAR, bold()]);

    // Only the weight differs, and that alone must reshape both styles.
    let normal = terminal_font(FontWeight::NORMAL);
    assert_eq!(
        Font {
            weight: FontWeight::MEDIUM,
            ..normal.clone()
        },
        medium
    );
    painter.borrow_mut().configure(&normal);
    assert_eq!(
        painter.borrow().glyphs.len(),
        0,
        "a new weight discards old glyphs"
    );
    assert!(painter.borrow().cell_width.is_none(), "and the cell width");
    draw(normal.clone());
    assert_eq!(painter.borrow().config.as_ref(), Some(&normal));
    assert_eq!(cached_styles(&painter.borrow()), [REGULAR, bold()]);
}
