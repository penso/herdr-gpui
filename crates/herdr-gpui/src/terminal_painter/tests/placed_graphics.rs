use super::*;

#[cfg(feature = "integration-test")]
#[gpui::test]
fn terminal_graphics_bypass_fonts_but_keep_decorations_and_skip_cells(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    cx.draw(Point::default(), size(px(800.), px(600.)), |_, _| {
        canvas(
            |_, _, _| (),
            |bounds, _, window, cx| {
                let mut frame = FrameData {
                    width: 5,
                    height: 1,
                    cells: vec![
                        CellData {
                            modifier: UNDERLINE | STRIKETHROUGH,
                            ..cell("▏")
                        },
                        CellData {
                            modifier: REVERSED,
                            ..cell("█")
                        },
                        CellData {
                            modifier: DIM,
                            ..cell("▀")
                        },
                        CellData {
                            modifier: HIDDEN,
                            ..cell("┼")
                        },
                        CellData {
                            skip: true,
                            ..cell("█")
                        },
                    ],
                    cursor: None,
                    hyperlinks: vec![],
                    graphics: vec![],
                };
                let mut painter = TerminalPainter::default();
                for family in ["Menlo", "Courier"] {
                    painter.set_appearance(21.35, 30.5, Theme::default());
                    let before = *cx.default_global::<crate::performance::Counts>();
                    painter.paint_frame(
                        &frame,
                        bounds.origin,
                        None,
                        12.81,
                        &font(family),
                        &[],
                        &[],
                        None,
                        window,
                        cx,
                    );
                    let after = cx.default_global::<crate::performance::Counts>();
                    assert_eq!(painter.glyphs.len(), 0);
                    assert_eq!(after.shapes, before.shapes);
                    assert_eq!(after.glyphs, before.glyphs);
                    assert_eq!(after.decorations - before.decorations, 2);
                    let backgrounds = background_spans(&frame.cells, &painter.theme).count();
                    assert_eq!(after.quads - before.quads, backgrounds + 7);
                }
                frame.cells[0] = cell("a");
                painter.paint_frame(
                    &frame,
                    bounds.origin,
                    None,
                    12.81,
                    &font("Menlo"),
                    &[],
                    &[],
                    None,
                    window,
                    cx,
                );
                assert_eq!(
                    painter.glyphs.len(),
                    1,
                    "ordinary text still uses the glyph cache"
                );
            },
        )
        .size_full()
    });
}

#[gpui::test]
fn placed_images_decode_off_thread_then_paint_in_z_order(cx: &mut TestAppContext) {
    use herdr_client::{
        SurfaceImages,
        protocol::{
            SurfaceGraphicsAsset, SurfaceGraphicsAssetKey, SurfaceGraphicsFormat,
            SurfaceGraphicsPlacement, SurfaceGraphicsSource, SurfaceGraphicsTarget,
        },
    };
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    let painter = std::rc::Rc::new(std::cell::RefCell::new(TerminalPainter::default()));
    painter
        .borrow_mut()
        .set_appearance(14., 20., Theme::default());
    let key = |image_id, target| SurfaceGraphicsAssetKey {
        source: SurfaceGraphicsSource::Terminal { target, image_id },
        image_width: 1,
        image_height: 1,
        format: SurfaceGraphicsFormat::Rgba,
        data_len: 4,
        data_fingerprint: u64::from(image_id),
    };
    let pane = || SurfaceGraphicsTarget::Pane {
        pane_id: "p1".into(),
    };
    let (above, below) = (key(1, pane()), key(2, pane()));
    let popup = key(
        3,
        SurfaceGraphicsTarget::Popup {
            terminal_id: "t".into(),
        },
    );
    let images: Arc<SurfaceImages> = Arc::new(
        [&above, &below, &popup]
            .into_iter()
            .map(|key| SurfaceGraphicsAsset {
                key: key.clone(),
                data: vec![1, 2, 3, 255],
            })
            .collect(),
    );
    let place = |key: &SurfaceGraphicsAssetKey, x, z| SurfaceGraphicsPlacement {
        asset: key.clone(),
        logical_placement_id: 1,
        x,
        y: 0,
        cols: 1,
        rows: 1,
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        x_offset: 0,
        y_offset: 0,
        z,
        scrollback_offset: 0,
    };
    // Scene order is not paint order: z decides.
    let placements: Arc<[SurfaceGraphicsPlacement]> = Arc::from([
        place(&above, 0, 5),
        place(&below, 1, -1),
        place(&popup, 2, 0),
    ]);
    let frame = FrameData {
        width: 3,
        height: 1,
        cells: vec![cell("x"); 3],
        cursor: None,
        hyperlinks: vec![],
        graphics: vec![],
    };
    let draw = |cx: &mut VisualTestContext| {
        let (painter, images, placements, frame) = (
            painter.clone(),
            images.clone(),
            placements.clone(),
            frame.clone(),
        );
        cx.draw(Point::default(), size(px(800.), px(600.)), |_, _| {
            canvas(
                |_, _, _| (),
                move |_, _, window, cx| {
                    painter.borrow_mut().paint_frame(
                        &frame,
                        point(px(10.), px(10.)),
                        None,
                        10.,
                        &font("Menlo"),
                        &[],
                        &[],
                        Some(PlacedImages {
                            placements: &placements,
                            images: &images,
                            target: ImageTarget::Main,
                        }),
                        window,
                        cx,
                    );
                },
            )
            .size_full()
        });
    };
    draw(cx);
    assert!(
        painter.borrow().painted_images.is_empty(),
        "nothing paints before its decode finishes"
    );
    cx.run_until_parked();
    draw(cx);
    let cell_at = |x: f32| Bounds::new(point(px(x), px(10.)), size(px(10.), px(20.)));
    // The popup's image belongs to the popup frame, not the main grid.
    assert_eq!(
        painter.borrow().painted_images,
        [(-1, cell_at(20.)), (5, cell_at(10.))]
    );
}
