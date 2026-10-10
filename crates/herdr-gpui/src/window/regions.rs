//! The terminal grid painted as one cached view per pane, plus one for the
//! cells around them, so an update repaints only the regions whose cells
//! changed. One busy pane no longer reshapes every other pane on screen.
//! A pane mid-slide (`smooth_scroll`) repaints every frame until it rests.

use super::HerdrWindow;
use crate::{
    config::Theme,
    smooth_scroll::Slide,
    terminal_painter::{self, Highlight, Layer, Part, Span, TerminalPainter},
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
    slide: Option<Sliding>,
}

/// A region's slide with its cells split once, at the pane's inner edge, for
/// every layer to paint.
#[derive(Clone)]
struct Sliding {
    slide: Slide,
    /// The cells inside the pane's inner rectangle, row by row.
    inside: Vec<Span>,
    outside: Vec<Span>,
}

impl Sliding {
    fn new(slide: Slide, area: &[Span]) -> Self {
        let (inside, outside) = split(area, slide.rect);
        Self {
            slide,
            inside,
            outside,
        }
    }

    /// The cells inside the pane on `rows`.
    fn rows(&self, rows: Range<u16>) -> &[Span] {
        let start = self.inside.partition_point(|span| span.row < rows.start);
        let end = self.inside.partition_point(|span| span.row < rows.end);
        &self.inside[start..end]
    }
}

impl Region {
    /// Whether `next` paints exactly what this region already painted. A
    /// moving slide never does; a resting one does while it stays put.
    fn paints_like(&self, next: &Self) -> bool {
        let slide = match (&self.slide, &next.slide) {
            (None, None) => true,
            (Some(old), Some(new)) => old.slide.paints_like(&new.slide),
            _ => false,
        };
        slide
            && self.area == next.area
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

/// One layer of one region of the grid. Its scene is replayed while the
/// region paints the same cells; a change replaces the view instead of
/// notifying it.
pub(crate) struct RegionView {
    painter: Rc<RefCell<TerminalPainter>>,
    region: Rc<Region>,
    layer: Layer,
}

/// A region's views, one per layer in `Layer::ALL` order.
pub(crate) type RegionLayers = [Entity<RegionView>; 3];

impl Render for RegionView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let painter = self.painter.clone();
        let region = self.region.clone();
        let layer = self.layer;
        canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                let mut painter = painter.borrow_mut();
                let look = &region.look;
                let mut paint = |frame: &FrameData,
                                 origin: Point<Pixels>,
                                 available: Option<Size<Pixels>>,
                                 highlights: &[Highlight],
                                 panes: &[PaneSurfacePane],
                                 area: &[Span],
                                 window: &mut Window| {
                    painter.paint_frame(
                        frame,
                        origin,
                        available,
                        look.cell_width,
                        &look.font,
                        highlights,
                        panes,
                        Some(Part { area, layer }),
                        None,
                        window,
                        cx,
                    );
                };
                let surface = &region.surface;
                let Some(sliding) = &region.slide else {
                    paint(
                        &surface.frame,
                        bounds.origin,
                        Some(bounds.size),
                        &region.highlights,
                        &surface.panes,
                        &region.area,
                        window,
                    );
                    return;
                };
                // The pane's border stays put while its content slides inside
                // it. Backgrounds paint at rest under the slide, so the
                // sub-cell remainder past the grid's last row stays filled.
                let slide = &sliding.slide;
                let rect = slide.rect;
                let still = match layer {
                    Layer::Backgrounds => &region.area,
                    Layer::Text | Layer::Decorations => &sliding.outside,
                };
                paint(
                    &surface.frame,
                    bounds.origin,
                    Some(bounds.size),
                    &region.highlights,
                    &surface.panes,
                    still,
                    window,
                );
                let (cell_width, cell_height) = (look.cell_width, look.cell_height);
                // Whole device pixels, so every glyph rasterizes as it does at rest.
                let scale = window.scale_factor();
                let offset = (slide.offset * cell_height * scale).round() / scale;
                let mask = Bounds::new(
                    bounds.origin
                        + point(
                            px(f32::from(rect.x) * cell_width),
                            px(f32::from(rect.y) * cell_height),
                        ),
                    size(
                        px(f32::from(rect.width) * cell_width),
                        px(f32::from(rect.height) * cell_height),
                    ),
                );
                window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                    // Each earlier frame fills the edge rows the newer ones lack.
                    let mut filled = 0;
                    for (earlier, shift) in &slide.behind {
                        let rows = u16::try_from(shift.abs_diff(filled))
                            .unwrap_or(rect.height)
                            .min(rect.height);
                        filled = *shift;
                        let bottom = rect.y.saturating_add(rect.height);
                        let band = match *shift > 0 {
                            true => bottom - rows..bottom,
                            false => rect.y..rect.y + rows,
                        };
                        let y = offset + *shift as f32 * cell_height;
                        paint(
                            &earlier.frame,
                            bounds.origin + point(px(0.), px(y)),
                            None,
                            &[],
                            &[],
                            sliding.rows(band),
                            window,
                        );
                    }
                    paint(
                        &surface.frame,
                        bounds.origin + point(px(0.), px(offset)),
                        None,
                        &region.highlights,
                        &surface.panes,
                        &sliding.inside,
                        window,
                    );
                });
            },
        )
        .size_full()
    }
}

/// The parts of `area` inside `rect`, and the parts outside it.
fn split(area: &[Span], rect: SurfaceRect) -> (Vec<Span>, Vec<Span>) {
    let rows = rect.y..rect.y.saturating_add(rect.height);
    let columns = rect.x..rect.x.saturating_add(rect.width);
    let (mut inside, mut outside) = (Vec::new(), Vec::new());
    for span in area {
        let start = span.columns.start.max(columns.start);
        let end = span.columns.end.min(columns.end);
        if !rows.contains(&span.row) || start >= end {
            outside.push(span.clone());
            continue;
        }
        inside.push(Span {
            row: span.row,
            columns: start..end,
        });
        for columns in [span.columns.start..start, end..span.columns.end] {
            if !columns.is_empty() {
                outside.push(Span {
                    row: span.row,
                    columns,
                });
            }
        }
    }
    (inside, outside)
}

impl HerdrWindow {
    /// The cached views painting `surface`'s cells, bottom first, or none when
    /// the grid must paint as a whole. Each covers the full grid bounds and
    /// paints one layer of only its own cells. Every region's backgrounds
    /// come before any region's text, and all text before any decoration, as
    /// in a whole-grid paint: a glyph overhanging its region into a border or
    /// a neighbouring pane stays visible.
    ///
    /// Images stay whole-frame: a texture no paint looks up is released while
    /// a replayed scene would still sample it.
    pub(super) fn terminal_regions(
        &mut self,
        surface: Option<&Arc<PaneSurfaceFrame>>,
        highlights: &[Highlight],
        mut slide: Option<Slide>,
        look: &Look,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(surface) = surface.filter(|surface| surface.graphics.placements.is_empty()) else {
            self.regions.clear();
            return Vec::new();
        };
        let mut previous = std::mem::take(&mut self.regions);
        for (owner, area) in partition(&surface.frame, &surface.panes) {
            let slide = match &owner {
                Owner::Pane(id) => slide.take_if(|slide| slide.pane_id == *id),
                Owner::Chrome => None,
            }
            .map(|slide| Sliding::new(slide, &area));
            let region = Rc::new(Region {
                owner,
                surface: surface.clone(),
                highlights: terminal_painter::clip(highlights, &area),
                area,
                look: look.clone(),
                slide,
            });
            let kept = previous
                .iter()
                .position(|views| views[0].read(cx).region.owner == region.owner)
                .map(|index| previous.swap_remove(index))
                .filter(|views| views[0].read(cx).region.paints_like(&region));
            // Notifying a view while the window draws only marks it for the
            // next frame, so a changed region takes a fresh view, which has
            // no scene to replay. An unchanged one adopts the new surface
            // without being invalidated, releasing the old frame.
            let views = match kept {
                Some(views) => {
                    for view in &views {
                        view.update(cx, |view, _| view.region = region.clone());
                    }
                    views
                }
                None => Layer::ALL.map(|layer| {
                    let painter = self.painter.clone();
                    let region = region.clone();
                    cx.new(|_| RegionView {
                        painter,
                        region,
                        layer,
                    })
                }),
            };
            self.regions.push(views);
        }
        (0..Layer::ALL.len())
            .flat_map(|layer| self.regions.iter().map(move |views| &views[layer]))
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
mod tests;
