//! The terminal grid painted as one cached view per pane, plus one for the
//! cells around them, so an update repaints only the regions whose cells
//! changed. One busy pane no longer reshapes every other pane on screen.

use super::HerdrWindow;
use crate::{
    config::Theme,
    terminal_painter::{self, Highlight, Span, TerminalPainter},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::{FrameData, PaneSurfaceFrame, PaneSurfacePane, SurfaceRect};
use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

/// Whose cells a region paints.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Owner {
    Pane(String),
    /// Borders, splits, and any other cell no pane covers.
    Chrome,
}

/// Everything besides the cells that changes how a region paints.
#[derive(Clone, PartialEq)]
pub(super) struct Look {
    pub(super) font: Font,
    pub(super) font_size: f32,
    pub(super) cell_width: f32,
    pub(super) cell_height: f32,
    pub(super) theme: Theme,
}

#[derive(Clone)]
struct Region {
    owner: Owner,
    surface: Arc<PaneSurfaceFrame>,
    area: Vec<Span>,
    highlights: Vec<Highlight>,
    look: Look,
}

impl Region {
    /// Whether `next` paints exactly what this region already painted.
    fn paints_like(&self, next: &Self) -> bool {
        self.area == next.area
            && self.highlights == next.highlights
            && self.look == next.look
            && (Arc::ptr_eq(&self.surface, &next.surface)
                || same_cells(&self.surface, &next.surface, &self.area))
    }
}

/// Whether both surfaces paint the same thing inside `area`: cells, cursor,
/// and the scrollbars that cross it.
fn same_cells(old: &PaneSurfaceFrame, new: &PaneSurfaceFrame, area: &[Span]) -> bool {
    let cursor = |frame: &FrameData| {
        frame
            .cursor
            .as_ref()
            .filter(|c| c.visible && terminal_painter::covers(area, c.x, c.y))
            .cloned()
    };
    let bars = |surface: &PaneSurfaceFrame| {
        surface
            .panes
            .iter()
            .filter_map(|pane| Some((pane.scrollbar_rect?, pane.scroll)))
            .filter(|(rect, _)| crosses(area, *rect))
            .collect::<Vec<_>>()
    };
    let (a, b) = (&old.frame, &new.frame);
    a.width == b.width
        && a.height == b.height
        && terminal_painter::cell_ranges(a, area)
            .all(|range| a.cells[range.clone()] == b.cells[range])
        && cursor(a) == cursor(b)
        && bars(old) == bars(new)
}

/// Whether any cell of `rect` lies in `area`.
fn crosses(area: &[Span], rect: SurfaceRect) -> bool {
    let rows = u32::from(rect.y)..u32::from(rect.y) + u32::from(rect.height);
    let right = u32::from(rect.x) + u32::from(rect.width);
    area.iter().any(|span| {
        rows.contains(&u32::from(span.row))
            && u32::from(span.columns.start) < right
            && rect.x < span.columns.end
    })
}

/// Splits the frame's cells among its panes, each pane owning the cells of
/// its rectangle that no earlier pane claimed, and the chrome the rest. Every
/// cell belongs to exactly one region, so none is painted twice.
fn partition(frame: &FrameData, panes: &[PaneSurfacePane]) -> Vec<(Owner, Vec<Span>)> {
    let mut regions: Vec<(Owner, Vec<Span>)> = panes
        .iter()
        .map(|pane| (Owner::Pane(pane.pane_id.clone()), Vec::new()))
        .chain([(Owner::Chrome, Vec::new())])
        .collect();
    let mut claimed: Vec<Range<u16>> = Vec::new();
    for row in 0..frame.height {
        claimed.clear();
        for (index, pane) in panes.iter().enumerate() {
            let rect = pane.rect;
            if row < rect.y || u32::from(row) >= u32::from(rect.y) + u32::from(rect.height) {
                continue;
            }
            let wanted =
                rect.x.min(frame.width)..rect.x.saturating_add(rect.width).min(frame.width);
            let mut start = wanted.start;
            for taken in &claimed {
                if taken.end <= start || taken.start >= wanted.end {
                    continue;
                }
                if taken.start > start {
                    regions[index].1.push(Span {
                        row,
                        columns: start..taken.start,
                    });
                }
                start = start.max(taken.end);
            }
            if start < wanted.end {
                regions[index].1.push(Span {
                    row,
                    columns: start..wanted.end,
                });
            }
            if wanted.start < wanted.end {
                let at = claimed.partition_point(|taken| taken.start < wanted.start);
                claimed.insert(at, wanted);
            }
        }
        let chrome = &mut regions[panes.len()].1;
        let mut start = 0;
        for taken in &claimed {
            if taken.start > start {
                chrome.push(Span {
                    row,
                    columns: start..taken.start,
                });
            }
            start = start.max(taken.end);
        }
        if start < frame.width {
            chrome.push(Span {
                row,
                columns: start..frame.width,
            });
        }
    }
    regions.retain(|(_, area)| !area.is_empty());
    regions
}

/// One region of the grid. Its scene is replayed while the region paints
/// the same cells; a change replaces the view instead of notifying it.
pub(crate) struct RegionView {
    painter: Rc<RefCell<TerminalPainter>>,
    region: Region,
}

impl Render for RegionView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let painter = self.painter.clone();
        let region = self.region.clone();
        canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                painter.borrow_mut().paint_frame(
                    &region.surface.frame,
                    bounds.origin,
                    Some(bounds.size),
                    region.look.cell_width,
                    &region.look.font,
                    &region.highlights,
                    &region.surface.panes,
                    Some(&region.area),
                    None,
                    window,
                    cx,
                );
            },
        )
        .size_full()
    }
}

impl HerdrWindow {
    /// The cached views painting `surface`'s cells, bottom first, or none when
    /// the grid must paint as a whole. Each covers the full grid bounds and
    /// paints only its own cells.
    ///
    /// Images stay whole-frame: a texture no paint looks up is released while
    /// a replayed scene would still sample it.
    pub(super) fn terminal_regions(
        &mut self,
        surface: Option<&Arc<PaneSurfaceFrame>>,
        highlights: &[Highlight],
        look: &Look,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(surface) = surface.filter(|surface| surface.graphics.placements.is_empty()) else {
            self.regions.clear();
            return Vec::new();
        };
        let mut previous = std::mem::take(&mut self.regions);
        for (owner, area) in partition(&surface.frame, &surface.panes) {
            let region = Region {
                owner,
                surface: surface.clone(),
                highlights: terminal_painter::clip(highlights, &area),
                area,
                look: look.clone(),
            };
            let kept = previous
                .iter()
                .position(|view| view.read(cx).region.owner == region.owner)
                .map(|index| previous.swap_remove(index))
                .filter(|view| view.read(cx).region.paints_like(&region));
            // Notifying a view while the window draws only marks it for the
            // next frame, so a changed region takes a fresh view, which has
            // no scene to replay. An unchanged one adopts the new surface
            // without being invalidated, releasing the old frame.
            let view = match kept {
                Some(view) => {
                    view.update(cx, |view, _| view.region = region);
                    view
                }
                None => {
                    let painter = self.painter.clone();
                    cx.new(|_| RegionView { painter, region })
                }
            };
            self.regions.push(view);
        }
        self.regions
            .iter()
            .map(|view| {
                view.clone()
                    .cached(
                        StyleRefinement::default()
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                    )
                    .into_any_element()
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{HerdrWindow, Owner, partition};
    use crate::sidebar::layout_tests::fixture_window;
    use gpui::{Entity, EntityId, TestAppContext, VisualTestContext};
    use herdr_client::protocol::{
        CellData, CursorState, FrameData, PaneSurfaceFrame, PaneSurfacePane, SurfaceRect,
    };
    use std::sync::Arc;

    fn rect(x: u16, y: u16, width: u16, height: u16) -> SurfaceRect {
        SurfaceRect {
            x,
            y,
            width,
            height,
        }
    }

    fn pane(id: &str, rect: SurfaceRect) -> PaneSurfacePane {
        PaneSurfacePane {
            pane_id: id.into(),
            content_revision: 1,
            rect,
            inner_rect: rect,
            scrollbar_rect: None,
            scroll: None,
            focused: false,
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            alternate_screen_active: false,
            pixel_width: 0,
            pixel_height: 0,
        }
    }

    fn frame(rows: &[&str]) -> FrameData {
        let width = rows[0].chars().count() as u16;
        FrameData {
            width,
            height: rows.len() as u16,
            cells: rows
                .iter()
                .flat_map(|row| row.chars())
                .map(|symbol| CellData {
                    symbol: symbol.to_string(),
                    fg: 0,
                    bg: 0,
                    modifier: 0,
                    skip: false,
                    hyperlink: None,
                })
                .collect(),
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        }
    }

    /// Each cell's owner, row by row, as the partition assigns it.
    fn owners(frame: &FrameData, panes: &[PaneSurfacePane]) -> Vec<Vec<Option<Owner>>> {
        let mut grid = vec![vec![None; usize::from(frame.width)]; usize::from(frame.height)];
        for (owner, area) in partition(frame, panes) {
            for span in area {
                for x in span.columns {
                    let cell = &mut grid[usize::from(span.row)][usize::from(x)];
                    assert!(cell.is_none(), "cell {x},{} painted twice", span.row);
                    *cell = Some(owner.clone());
                }
            }
        }
        grid
    }

    #[test]
    fn every_cell_belongs_to_one_region_and_earlier_panes_win_overlaps() {
        let frame = frame(&["aaaa|bbbb", "aaaa|bbbb", "---------"]);
        let a = Owner::Pane("a".into());
        let b = Owner::Pane("b".into());
        // Side by side, with a border column and a status row between them.
        let grid = owners(
            &frame,
            &[pane("a", rect(0, 0, 4, 2)), pane("b", rect(5, 0, 4, 2))],
        );
        for row in &grid[..2] {
            assert!(row[..4].iter().all(|o| o.as_ref() == Some(&a)));
            assert_eq!(row[4], Some(Owner::Chrome));
            assert!(row[5..].iter().all(|o| o.as_ref() == Some(&b)));
        }
        assert!(grid[2].iter().all(|o| o.as_ref() == Some(&Owner::Chrome)));
        // A pane over another, and one reaching past the grid, still paint
        // each cell once: the first listed keeps the shared cells.
        let grid = owners(
            &frame,
            &[pane("a", rect(2, 1, 4, 5)), pane("b", rect(0, 0, 20, 2))],
        );
        assert_eq!(grid[0], vec![Some(b.clone()); 9]);
        assert_eq!(
            grid[1],
            [&b, &b, &a, &a, &a, &a, &b, &b, &b].map(|o| Some(o.clone()))
        );
        assert!(grid[2][2..6].iter().all(|o| o.as_ref() == Some(&a)));
        assert!(
            grid[2][..2]
                .iter()
                .all(|o| o.as_ref() == Some(&Owner::Chrome))
        );
    }

    /// Draws `surface` and returns the region views in owner order.
    fn draw(
        view: &Entity<HerdrWindow>,
        surface: &PaneSurfaceFrame,
        cx: &mut VisualTestContext,
    ) -> Vec<(Owner, EntityId)> {
        view.update(cx, |view, _| {
            let snapshot = view.live.snapshot.as_ref().unwrap();
            let mut surface = surface.clone();
            surface.boot_id = snapshot.boot_id.clone();
            surface.projection_revision = snapshot.revision;
            view.live.surface = Some(Arc::new(surface));
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.read_with(cx, |view, cx| {
            view.regions
                .iter()
                .map(|region| (region.read(cx).region.owner.clone(), region.entity_id()))
                .collect()
        })
    }

    #[gpui::test]
    fn only_regions_whose_paint_changed_are_replaced(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        let mut surface = PaneSurfaceFrame {
            boot_id: String::new(),
            projection_revision: 0,
            surface_revision: 1,
            frame: frame(&["aaaa|bbbb", "aaaa|bbbb"]),
            panes: vec![pane("a", rect(0, 0, 4, 2)), pane("b", rect(5, 0, 4, 2))],
            splits: vec![],
            popup: None,
            graphics: Default::default(),
        };
        let first = draw(&view, &surface, cx);
        let owners: Vec<_> = first.iter().map(|(owner, _)| owner.clone()).collect();
        assert_eq!(
            owners,
            [
                Owner::Pane("a".into()),
                Owner::Pane("b".into()),
                Owner::Chrome
            ]
        );
        // The same cells in a new frame repaint nothing.
        assert_eq!(draw(&view, &surface, cx), first);
        // Output in one pane replaces only that pane's region.
        surface.frame.cells[6].symbol = "x".into();
        let second = draw(&view, &surface, cx);
        assert_eq!(second[0], first[0]);
        assert_ne!(second[1], first[1]);
        assert_eq!(second[2], first[2]);
        // The cursor leaving one pane for another changes both.
        surface.frame.cursor = Some(CursorState {
            x: 1,
            y: 1,
            visible: true,
            shape: 0,
        });
        let third = draw(&view, &surface, cx);
        assert_ne!(third[0], second[0]);
        assert_eq!(third[1..], second[1..]);
        surface.frame.cursor.as_mut().unwrap().x = 6;
        let fourth = draw(&view, &surface, cx);
        assert_ne!(fourth[0], third[0]);
        assert_ne!(fourth[1], third[1]);
        assert_eq!(fourth[2], third[2]);
    }
}
