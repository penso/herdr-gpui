//! The connected surface of a layout that draws one: the chosen tab's body,
//! painted behind the strip's tabs so it can slide between them, whose foot
//! curves out into the group's content, and the inset card that content
//! sits in, with the chrome around it.

use crate::{
    HerdrWindow,
    browser::{
        Pick, Slot,
        tab_body::{Cells, List, Span},
    },
    tab_layout::Connected,
};
use gpui::{prelude::*, *};

/// Where the strip sits against the content it joins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Above,
    Below,
}

impl HerdrWindow {
    /// The layout's connected shape, when it draws one.
    pub(super) fn connected(&self) -> Option<Connected> {
        self.tab_layout().connected()
    }

    /// `slot`'s body for this frame, with `pick` chosen: the canvas that
    /// paints it, laid over the tab list and clipped to it, and the recorder
    /// the chosen tab carries to place it. `None` for a layout
    /// without one.
    pub(super) fn tab_body(
        &mut self,
        slot: Slot,
        pick: Option<&Pick>,
        side: Side,
        window: &mut Window,
    ) -> Option<(AnyElement, Placer)> {
        let shape = self.connected()?;
        let now = crate::motion::now();
        let frame = self.browser.tab_body.frame(slot.id, pick, now);
        if frame.moving(now) {
            window.request_animation_frame();
        }
        let color = self.fill().ground();
        let list = frame.cells.list.clone();
        let drawn = frame.cells.drawn.clone();
        let canvas = canvas(
            move |bounds, window, _| {
                list.set(Some(List {
                    bounds,
                    clip: window.content_mask().bounds,
                }));
            },
            move |bounds, _, window, _| {
                if let Some(path) = drawn
                    .get()
                    .and_then(|span| body_path(bounds, span, &shape, side))
                {
                    window.paint_path(path, color);
                }
            },
        )
        .size_full();
        let canvas = div()
            .debug_selector(move || slot.selector("tab-body"))
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .overflow_hidden()
            .child(canvas)
            .into_any_element();
        let placer = Placer {
            cells: frame.cells,
            from: frame.from,
            now,
            shoulder: shape.shoulder,
        };
        Some((canvas, placer))
    }

    /// `body` in the card a connected layout sits a group's content in,
    /// the chrome painted around it, its corners rounded except where the
    /// strip's body reaches one. `strip` says where the strip sits, if the
    /// group shows one.
    pub(super) fn content_card(
        &self,
        slot: Slot,
        body: AnyElement,
        strip: Option<Side>,
    ) -> AnyElement {
        let Some(shape) = self.connected() else {
            return body;
        };
        let chrome = self.fill().panel(self.theme.surface);
        let cells = strip.and_then(|_| self.browser.tab_body.cells(slot.id));
        let inset = px(shape.inset);
        let frame = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let card = card_bounds(bounds, &shape, strip);
                let reach = cells.as_ref().and_then(|cells| cells.visible.get());
                let near = |corner: Pixels, distance: fn(Pixels, (Pixels, Pixels)) -> Pixels| {
                    reach.map_or(shape.card_radius, |run| {
                        eased(f32::from(distance(corner, run)), shape.card_radius)
                    })
                };
                let leading = near(card.left(), |edge, (left, _)| left - edge);
                let trailing = near(card.right(), |edge, (_, right)| edge - right);
                let radii = match strip {
                    Some(Side::Above) => [leading, trailing, shape.card_radius, shape.card_radius],
                    Some(Side::Below) => [shape.card_radius, shape.card_radius, leading, trailing],
                    None => [shape.card_radius; 4],
                };
                #[cfg(test)]
                if let Some(cells) = &cells {
                    cells.corners.set(Some(radii));
                }
                for piece in margins(bounds, card) {
                    window.paint_quad(fill(piece, chrome));
                }
                for path in fillets(card, radii).into_iter().flatten() {
                    window.paint_path(path, chrome);
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        div()
            .debug_selector(move || slot.selector("content-card"))
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .px(inset)
            .map(|card| match strip {
                Some(Side::Above) => card.pb(inset),
                Some(Side::Below) => card.pt(inset),
                None => card.py(inset),
            })
            .child(body)
            // Last, so the rounded corners trim what the group shows.
            .child(frame)
            .into_any_element()
    }
}

/// What places a strip's body for one frame, from the chosen tab.
#[derive(Clone, Debug)]
pub(super) struct Placer {
    cells: Cells,
    from: Option<(Span, std::time::Instant)>,
    now: std::time::Instant,
    shoulder: f32,
}

impl Placer {
    /// A canvas the chosen tab carries, which places the body over it as
    /// it prepaints: after the tab list's own canvas, and before anything
    /// paints, the card under or over the strip included.
    pub(super) fn target(&self) -> impl IntoElement {
        let placer = self.clone();
        canvas(move |bounds, _, _| placer.place(bounds), |_, _, _, _| {})
            .absolute()
            .top_0()
            .left_0()
            .size_full()
    }

    /// Places the body heading for a chosen tab drawn at `target`.
    fn place(&self, target: Bounds<Pixels>) {
        let Some(List { bounds: list, clip }) = self.cells.list.get() else {
            return;
        };
        let span = Span {
            left: f32::from(target.left() - list.left()),
            width: f32::from(target.size.width),
        }
        .from(self.from, self.now);
        self.cells.drawn.set(Some(span));
        let left = (list.left() + px(span.left - self.shoulder)).max(clip.left());
        let right = (list.left() + px(span.left + span.width + self.shoulder)).min(clip.right());
        self.cells
            .visible
            .set((right > left).then_some((left, right)));
    }
}

/// How round a card's corner draws whose edge lies `clearance` from the
/// strip's body: square where the body reaches it, easing to full `radius`
/// as the body moves a radius away, as 1git eases its own.
fn eased(clearance: f32, radius: f32) -> f32 {
    let k = (clearance / radius.max(1.)).clamp(0., 1.);
    radius * k * k * (3. - 2. * k)
}

/// The card within the content's `bounds`: the chrome kept on every side
/// but the strip's.
fn card_bounds(bounds: Bounds<Pixels>, shape: &Connected, strip: Option<Side>) -> Bounds<Pixels> {
    let inset = px(shape.inset);
    let top = if strip == Some(Side::Above) {
        px(0.)
    } else {
        inset
    };
    let bottom = if strip == Some(Side::Below) {
        px(0.)
    } else {
        inset
    };
    Bounds::from_corners(
        point(bounds.left() + inset, bounds.top() + top),
        point(bounds.right() - inset, bounds.bottom() - bottom),
    )
}

/// The chrome around `card`: up to four bands of `bounds` outside it.
fn margins(bounds: Bounds<Pixels>, card: Bounds<Pixels>) -> Vec<Bounds<Pixels>> {
    [
        Bounds::from_corners(bounds.origin, point(card.left(), bounds.bottom())),
        Bounds::from_corners(point(card.right(), bounds.top()), bounds.bottom_right()),
        Bounds::from_corners(point(card.left(), bounds.top()), card.top_right()),
        Bounds::from_corners(card.bottom_left(), point(card.right(), bounds.bottom())),
    ]
    .into_iter()
    .filter(|piece| piece.size.width > px(0.) && piece.size.height > px(0.))
    .collect()
}

/// The chrome inside each of `card`'s corners, outside its rounding, with
/// `radii` top left, top right, bottom left, bottom right. A square corner
/// has none.
fn fillets(card: Bounds<Pixels>, radii: [f32; 4]) -> [Option<Path<Pixels>>; 4] {
    let corners = [
        (card.origin, 1., 1.),
        (card.top_right(), -1., 1.),
        (card.bottom_left(), 1., -1.),
        (card.bottom_right(), -1., -1.),
    ];
    let mut paths = [None, None, None, None];
    for (slot, ((corner, x, y), radius)) in paths.iter_mut().zip(corners.into_iter().zip(radii)) {
        if radius < 0.5 {
            continue;
        }
        let r = px(radius);
        let mut path = PathBuilder::fill();
        path.move_to(corner);
        path.line_to(point(corner.x + r * x, corner.y));
        path.curve_to(point(corner.x, corner.y + r * y), corner);
        path.close();
        *slot = path.build().ok();
    }
    paths
}

/// The body over `span` of a tab list drawn in `bounds`, as 1git's
/// `connectedTabSurface` draws it: rounded corners away from the content,
/// and concave shoulders that flare out to meet it.
fn body_path(
    bounds: Bounds<Pixels>,
    span: Span,
    shape: &Connected,
    side: Side,
) -> Option<Path<Pixels>> {
    let height = f32::from(bounds.size.height);
    // Drawn for a strip above its content, foot at the bottom, and
    // mirrored for one below.
    let at = |x: f32, y: f32| {
        let y = match side {
            Side::Above => y,
            Side::Below => height - y,
        };
        point(bounds.left() + px(x), bounds.top() + px(y))
    };
    let (s, r, top) = (shape.shoulder, shape.radius, shape.top);
    let (left, right) = (span.left, span.left + span.width);
    let mut path = PathBuilder::fill();
    path.move_to(at(left - s, height));
    path.curve_to(at(left, height - s), at(left, height));
    path.line_to(at(left, top + r));
    path.curve_to(at(left + r, top), at(left, top));
    path.line_to(at(right - r, top));
    path.curve_to(at(right, top + r), at(right, top));
    path.line_to(at(right, height - s));
    path.curve_to(at(right + s, height), at(right, height));
    path.close();
    path.build().ok()
}

#[cfg(test)]
mod tests;
