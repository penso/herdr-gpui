//! Reordering devices by dragging their headers. A press lifts a header as it
//! lifts a workspace row; while it is lifted the list folds to its headers,
//! so a device with many rows moves as one line among the others, and the
//! headers it passes slide aside to open the gap it lands in. Releasing
//! there moves the device for this client (`endpoint::order`), Local
//! included.

use crate::{
    HerdrWindow,
    reorder::{Beside, LIFT_DELAY, LIFT_DISTANCE, SLIDE, Slide, preview, slot_for},
};
use gpui::{prelude::*, *};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};

/// A press on a device's header that may become a reorder.
pub(crate) struct DeviceDrag {
    endpoint: String,
    origin: Point<Pixels>,
    pointer: Point<Pixels>,
    /// How far below the header's top the press landed. The list folds when
    /// the header lifts, moving it, so the header follows the pointer from
    /// that spot rather than from where it was pressed.
    grab: Pixels,
    lifted: bool,
    /// The gap under the pointer and the move it makes, `None` over the
    /// carried header's own place.
    target: Option<(usize, Beside<String>)>,
    /// Whether a folded frame has been laid out: until then the list's
    /// geometry is the unfolded one and cannot place the headers.
    measured: Cell<bool>,
    /// Each header's slide, by endpoint ID. Render reads and advances them.
    slides: RefCell<HashMap<String, Slide>>,
    _timer: Task<()>,
}

impl DeviceDrag {
    pub(super) fn carries(&self, endpoint: &str) -> bool {
        self.lifted && self.endpoint == endpoint
    }
}

/// What the pointer handler needs from the folded list render laid out:
/// each movable header's unit, row, and shift, and the devices in order.
#[derive(Clone, Debug, Default)]
pub(super) struct DeviceDrop {
    rows: Vec<(usize, usize, Pixels)>,
    ids: Vec<String>,
    dragged: usize,
}

/// The folded list's headers while a device is lifted.
pub(super) struct DevicePlan {
    /// Each listed header's shift by endpoint index, and whether it is the
    /// carried one.
    shifts: HashMap<usize, (Pixels, bool)>,
    pub(super) drop: DeviceDrop,
    /// Whether a header is still sliding, or the fold still needs measuring.
    pub(super) sliding: bool,
}

impl DevicePlan {
    pub(super) fn shift(&self, endpoint: usize) -> (Pixels, bool) {
        self.shifts.get(&endpoint).copied().unwrap_or_default()
    }
}

/// Where row `row` of the spaces list sits in the window. The handle reports
/// where layout put it, before the list's scroll offset moves it.
fn window_top(scroll: &ScrollHandle, row: usize) -> Option<Pixels> {
    Some(scroll.bounds_for_item(row)?.top() + scroll.offset().y)
}

/// The gap a header carried with its top at `card_top`, in window
/// coordinates, points to against the geometry the folded list last laid out.
fn slot(drop: &DeviceDrop, scroll: &ScrollHandle, card_top: Pixels) -> Option<usize> {
    let mut spans = vec![None; drop.ids.len()];
    // Headers are measured where they rest, not where the preview slid them.
    for &(unit, row, shift) in &drop.rows {
        let (Some(top), Some(bounds)) = (window_top(scroll, row), scroll.bounds_for_item(row))
        else {
            continue;
        };
        let top = f32::from(top - shift);
        spans[unit] = Some((top, top + f32::from(bounds.size.height)));
    }
    let (top, bottom) = (*spans.get(drop.dragged)?)?;
    let card = f32::from(card_top);
    Some(slot_for(&spans, drop.dragged, (card, card + bottom - top)))
}

/// The move dropping into `slot` makes: before the device that holds it, or
/// after the last one. `None` for the gaps around the carried device.
fn landing(ids: &[String], dragged: usize, slot: usize) -> Option<Beside<String>> {
    if slot == dragged || slot == dragged + 1 {
        return None;
    }
    match ids.get(slot) {
        Some(id) => Some(Beside::Before(id.clone())),
        None => ids.last().cloned().map(Beside::After),
    }
}

impl HerdrWindow {
    /// Arms a reorder for a left press on device `index`'s header. `row` is
    /// the header's place in the spaces list.
    pub(super) fn press_device(
        &mut self,
        index: usize,
        row: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        // A device listed alone, by the filter or a search, has nowhere to
        // go, so its header stays a plain click.
        let devices = !self.config.layout.mode.lists_workspaces();
        let query = if devices && self.config.show_sidebar_search {
            self.device_tree_query(cx)
        } else {
            String::new()
        };
        if self.listed_headers(devices, &query).len() < 2
            || self.menu.page.is_some()
            || self.workspace_drag.is_some()
        {
            return;
        }
        let Some(endpoint) = self.endpoints.get(index) else {
            return;
        };
        let top = window_top(&self.sidebar_scroll[0], row).unwrap_or(position.y);
        let lift = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LIFT_DELAY).await;
            let _ = this.update(cx, |this, cx| this.lift_device(cx));
        });
        self.device_drag = Some(DeviceDrag {
            endpoint: endpoint.id.clone(),
            origin: position,
            pointer: position,
            grab: position.y - top,
            lifted: false,
            target: None,
            measured: Cell::new(false),
            slides: RefCell::default(),
            _timer: lift,
        });
    }

    fn lift_device(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = &mut self.device_drag
            && !drag.lifted
        {
            drag.lifted = true;
            self.hover = None;
            cx.notify();
        }
    }

    /// Each header the spaces list shows, by endpoint index, with its row in
    /// the folded list, where every header is one row. `devices` and `query`
    /// list the headers as render does.
    fn listed_headers(&self, devices: bool, query: &str) -> Vec<(usize, usize)> {
        self.displayed_devices()
            .into_iter()
            .filter(|&(index, endpoint)| {
                self.device_visible(&endpoint.id) && (!devices || self.device_listed(index, query))
            })
            .enumerate()
            .map(|(row, (index, _))| (index, row))
            .collect()
    }

    /// The folded list's headers for a lifted device, or `None` when no
    /// device is lifted or fewer than two can move. `devices` and `query`
    /// list the headers as render does.
    pub(super) fn device_plan(&self, devices: bool, query: &str) -> Option<DevicePlan> {
        let drag = self.device_drag.as_ref().filter(|drag| drag.lifted)?;
        let units = self.listed_headers(devices, query);
        let dragged = units
            .iter()
            .position(|&(index, _)| self.endpoints[index].id == drag.endpoint)?;
        if units.len() < 2 {
            return None;
        }
        let scroll = &self.sidebar_scroll[0];
        let measured = drag.measured.replace(true);
        let heights: Vec<f32> = units
            .iter()
            .map(|&(_, row)| {
                scroll
                    .bounds_for_item(row)
                    .filter(|_| measured)
                    .map_or(0., |bounds| f32::from(bounds.size.height))
            })
            .collect();
        let slot = drag.target.as_ref().map_or(dragged, |(slot, _)| *slot);
        let previewed = preview(&heights, dragged, slot);
        let now = crate::motion::now();
        let mut slides = drag.slides.borrow_mut();
        let mut plan = DevicePlan {
            shifts: HashMap::new(),
            drop: DeviceDrop {
                rows: Vec::new(),
                ids: Vec::new(),
                dragged,
            },
            sliding: !measured,
        };
        for (unit, &(index, row)) in units.iter().enumerate() {
            let id = &self.endpoints[index].id;
            let shift = if unit == dragged {
                // The header rests where the folded list put it; it paints
                // where the pointer holds it.
                let previous = slides.get(id).map_or(0., |slide| slide.to);
                let at = window_top(scroll, row)
                    .filter(|_| measured)
                    .map_or(0., |top| {
                        f32::from(drag.pointer.y - drag.grab - top) + previous
                    });
                slides.insert(
                    id.clone(),
                    Slide {
                        from: at,
                        to: at,
                        start: now,
                    },
                );
                at
            } else {
                let slide = slides.entry(id.clone()).or_insert(Slide {
                    from: 0.,
                    to: 0.,
                    start: now,
                });
                *slide = slide.toward(previewed[unit], now, SLIDE);
                plan.sliding |= slide.progress(now, SLIDE) < 1.;
                slide.at(now, SLIDE)
            };
            plan.shifts.insert(index, (px(shift), unit == dragged));
            plan.drop.rows.push((unit, row, px(shift)));
            plan.drop.ids.push(id.clone());
        }
        Some(plan)
    }

    /// A host header row in the spaces list at `row`: pressing it may lift
    /// the device, and while one is lifted it takes its place in the plan.
    pub(super) fn place_host_row(
        &self,
        list: Stateful<Div>,
        header: Stateful<Div>,
        (index, row): (usize, usize),
        plan: Option<&DevicePlan>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (shift, carried) = plan.map_or_else(Default::default, |plan| plan.shift(index));
        let header = header
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if event.click_count == 1 {
                        this.press_device(index, row, event.position, cx);
                    }
                }),
            )
            .when(shift != px(0.), |header| header.top(shift))
            .when(carried, |header| header.cursor_grabbing());
        if carried {
            // Painted last so it floats over the headers it passes.
            list.child(deferred(header).with_priority(1))
        } else {
            list.child(header)
        }
    }

    /// Follows the pointer, resolving a lifted header's gap against `drop`.
    /// Returns whether the move belongs to the drag.
    pub(super) fn move_device_drag(
        &mut self,
        position: Point<Pixels>,
        held: bool,
        drop: Option<&DeviceDrop>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.device_drag.as_mut() else {
            return false;
        };
        // A release outside the window never reaches the sidebar.
        if !held {
            let lifted = drag.lifted;
            self.device_drag = None;
            cx.notify();
            return lifted;
        }
        drag.pointer = position;
        let moved = position - drag.origin;
        if !drag.lifted && f32::from(moved.x.abs().max(moved.y.abs())) > LIFT_DISTANCE {
            self.lift_device(cx);
        }
        let scroll = self.sidebar_scroll[0].clone();
        let Some(drag) = self.device_drag.as_mut().filter(|drag| drag.lifted) else {
            return false;
        };
        drag.target = drop.and_then(|drop| {
            let slot = slot(drop, &scroll, position.y - drag.grab)?;
            Some((slot, landing(&drop.ids, drop.dragged, slot)?))
        });
        cx.notify();
        true
    }

    /// Ends the drag on release, moving the device if it was lifted over a
    /// gap. Returns whether it had lifted, in which case the release is not
    /// a click.
    pub(super) fn release_device_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.device_drag.take() else {
            return false;
        };
        if !drag.lifted {
            return false;
        }
        cx.notify();
        if let Some((_, to)) = &drag.target {
            let to = match to {
                Beside::Before(id) => Beside::Before(id.as_str()),
                Beside::After(id) => Beside::After(id.as_str()),
            };
            self.move_device(&drag.endpoint, to);
        }
        true
    }

    /// Drops a lifted header where it came from, and forgets a press that
    /// has not lifted yet, so its timer cannot lift it later. Returns whether
    /// one was lifted: only then is the key the drag's.
    pub(crate) fn cancel_device_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.device_drag.take() else {
            return false;
        };
        if drag.lifted {
            cx.notify();
        }
        drag.lifted
    }
}

#[cfg(test)]
mod tests;
