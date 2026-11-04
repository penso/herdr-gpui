use super::*;

/// Paints one row (the default background, a program's color, and the
/// theme's background chosen explicitly) and returns the quad colors.
fn row_quads(ground: Ground, cx: &mut TestAppContext) -> Vec<Background> {
    let theme = Theme::default();
    let cells = [0, 0x02000000 | 0x123456, 0x02000000 | theme.background]
        .into_iter()
        .map(|bg| CellData { bg, ..cell("a") })
        .collect();
    quads(cells, ground, cx)
}

/// Paints `cells` as one row and returns the quad colors.
fn quads(cells: Vec<CellData>, ground: Ground, cx: &mut TestAppContext) -> Vec<Background> {
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    let theme = Theme::default();
    cx.draw(Point::default(), size(px(100.), px(40.)), move |_, _| {
        canvas(
            |_, _, _| (),
            move |_, _, window, cx| {
                let frame = FrameData {
                    width: cells.len() as u16,
                    height: 1,
                    cells: cells.clone(),
                    cursor: None,
                    hyperlinks: vec![],
                    graphics: vec![],
                };
                let mut painter = TerminalPainter::default();
                painter.set_appearance(14., 20., theme.clone());
                painter.paint_frame(
                    &frame,
                    point(px(0.), px(0.)),
                    Some(size(px(10. * f32::from(frame.width)), px(20.))),
                    ground,
                    10.,
                    &font("Menlo"),
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
    cx.update(|window, _| {
        window
            .painted_quads()
            .iter()
            .map(|quad| quad.background)
            .collect()
    })
}

#[gpui::test]
fn a_backdrop_skips_only_the_themes_own_background(cx: &mut TestAppContext) {
    let background: Background = rgb(Theme::default().background).into();
    let chosen: Background = rgb(0x123456).into();
    // Opaque, every cell paints, the theme's color on both sides of the
    // program's.
    assert_eq!(
        row_quads(Ground::Opaque, cx),
        [background, chosen, background]
    );
    // Over a backdrop the ground already is the theme's default, so painting
    // it again would stack its alpha. A color the program chose stays solid,
    // even when it equals the theme's.
    assert_eq!(row_quads(Ground::Backdrop, cx), [chosen, background]);
}

#[gpui::test]
fn hidden_cells_draw_nothing_over_a_backdrop(cx: &mut TestAppContext) {
    // Hidden text draws in its background's color; over a skipped quad that
    // color would show, so neither its underline nor a block graphic paints.
    let hidden = |symbol| CellData {
        modifier: HIDDEN | UNDERLINE,
        ..cell(symbol)
    };
    let cells = vec![hidden("a"), hidden("\u{2588}")];
    let background: Background = rgb(Theme::default().background).into();
    // Opaque, they still paint, in the color of the background under them.
    let opaque = quads(cells.clone(), Ground::Opaque, cx);
    assert!(opaque.len() > 1 && opaque.iter().all(|quad| *quad == background));
    assert_eq!(quads(cells, Ground::Backdrop, cx), []);
}
