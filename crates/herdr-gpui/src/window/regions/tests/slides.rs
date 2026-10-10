use super::*;
use crate::{
    smooth_scroll::Slide,
    terminal_painter::Span,
    window::regions::{Region, Sliding, split},
};

fn span(row: u16, columns: std::ops::Range<u16>) -> Span {
    Span { row, columns }
}

fn surface(frame: FrameData, pane: PaneSurfacePane) -> Arc<PaneSurfaceFrame> {
    Arc::new(PaneSurfaceFrame {
        boot_id: "boot".into(),
        projection_revision: 1,
        surface_revision: 1,
        frame,
        panes: vec![pane],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    })
}

fn slide(rect: SurfaceRect, offset: f32, behind: Vec<(Arc<PaneSurfaceFrame>, i32)>) -> Slide {
    Slide {
        pane_id: "p".into(),
        rect,
        offset,
        behind,
    }
}

/// Pane `p`'s region of `surface`, sliding by `slide`.
fn region(surface: Arc<PaneSurfaceFrame>, slide: Option<Slide>) -> Region {
    let area = partition(&surface.frame, &surface.panes).remove(0).1;
    Region {
        owner: Owner::Pane("p".into()),
        slide: slide.map(|slide| Sliding::new(slide, &area)),
        area,
        surface,
        highlights: vec![],
        look: look(),
    }
}

#[test]
fn split_keeps_every_cell_on_exactly_one_side_of_the_rect() {
    let area = [span(0, 0..6), span(1, 0..6), span(2, 4..6), span(3, 0..6)];
    let (inside, outside) = split(&area, rect(1, 1, 3, 2));
    assert_eq!(inside, [span(1, 1..4)]);
    assert_eq!(
        outside,
        [
            span(0, 0..6),
            span(1, 0..1),
            span(1, 4..6),
            span(2, 4..6),
            span(3, 0..6)
        ]
    );
}

#[test]
fn a_band_paints_only_the_cells_inside_the_pane_on_its_rows() {
    let area: Vec<_> = (0..5).map(|row| span(row, 0..6)).collect();
    let sliding = Sliding::new(slide(rect(1, 1, 4, 3), 0., vec![]), &area);
    assert_eq!(sliding.rows(0..1), []);
    assert_eq!(sliding.rows(2..4), [span(2, 1..5), span(3, 1..5)]);
    assert_eq!(sliding.rows(3..9), [span(3, 1..5)]);
}

#[test]
fn a_region_replays_only_a_slide_drawn_at_the_same_offset() {
    let rect = rect(0, 0, 4, 1);
    let surface = surface(frame(&["aaaa"]), pane("p", rect));
    let at_rest = region(surface.clone(), None);
    let sliding = region(surface.clone(), Some(slide(rect, -0.5, vec![])));
    let moved = region(surface, Some(slide(rect, -0.25, vec![])));
    assert!(at_rest.paints_like(&at_rest));
    // A pane resting mid-row keeps its paint; a moving one repaints.
    assert!(sliding.paints_like(&sliding));
    assert!(!sliding.paints_like(&moved));
    // Coming to rest repaints once without the offset.
    assert!(!sliding.paints_like(&at_rest));
}

/// A sliding pane paints its border at rest, its rows from the current frame
/// offset inside the border, and only the uncovered band from the frames
/// behind: each cell once, with one cursor.
#[cfg(feature = "integration-test")]
#[gpui::test]
fn a_sliding_region_fills_only_its_uncovered_edge_from_earlier_frames(cx: &mut TestAppContext) {
    use crate::{terminal_painter::TerminalPainter, window::regions::RegionView};
    use gpui::{AppContext, Empty, IntoElement, ParentElement, Point, Styled, px, size};
    use std::{cell::RefCell, rc::Rc};

    let surface = |letter: &str| {
        let row = letter.repeat(4);
        let mut frame = frame(&[row.as_str(); 6]);
        frame.cursor = Some(CursorState {
            x: 1,
            y: 2,
            visible: true,
            shape: 0,
        });
        let mut pane = pane("p", rect(0, 0, 4, 6));
        pane.inner_rect = rect(0, 1, 4, 5);
        surface(frame, pane)
    };
    let (previous, current) = (surface("a"), surface("b"));
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    // Bands of 2 then 1 rows from two earlier frames; then one reversed.
    for (shifts, band) in [(vec![2, 3], 3), (vec![-1], 1)] {
        let offset = -shifts[shifts.len() - 1] as f32 / 2.;
        let behind = shifts.iter().map(|s| (previous.clone(), *s)).collect();
        let slide = slide(current.panes[0].inner_rect, offset, behind);
        let region = Rc::new(region(current.clone(), Some(slide)));
        let painter = Rc::new(RefCell::new(TerminalPainter::default()));
        let before = cx.update(|_, cx| *cx.default_global::<crate::performance::Counts>());
        cx.draw(Point::default(), size(px(800.), px(600.)), |_, cx| {
            let layers = Layer::ALL.map(|layer| {
                let (painter, region) = (painter.clone(), region.clone());
                cx.new(|_| RegionView {
                    painter,
                    region,
                    layer,
                })
            });
            gpui::div()
                .size_full()
                .children(layers.map(|view| view.into_any_element()))
        });
        let after = cx.update(|_, cx| *cx.default_global::<crate::performance::Counts>());
        // The row above the pane's content, its five rows, and the band.
        assert_eq!(after.glyphs - before.glyphs, (1 + 5 + band) * 4);
        assert_eq!(after.decorations - before.decorations, 1, "one cursor");
    }
}
