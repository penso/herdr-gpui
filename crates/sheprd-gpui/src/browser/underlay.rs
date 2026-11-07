//! What the window paints where a page shows, on macOS: nothing.
//!
//! A page lies beneath the window's drawing ([`crate::browser::stacking`]),
//! so every layer under it leaves the page's rectangle unpainted. GPUI only
//! blends new pixels over old ones and cannot clear what a parent painted,
//! so each background under a page paints around it instead: the window's
//! ground or picture, a zen column's ground, a group's body, and a browser
//! tab's own background. Anything the window draws over a page, such as a
//! menu or a toast, paints as usual and shows over the page.
use {
    crate::browser::page::Drawn,
    gpui::{
        AnyElement, App, Bounds, Corners, Element, ElementId, GlobalElementId, Hsla,
        InspectorElementId, IntoElement, LayoutId, ObjectFit, Pixels, RenderImage, Styled as _,
        Window, canvas, fill, point, px,
    },
    std::sync::Arc,
};

/// `area` without the pages in `pages`, as rectangles that do not overlap.
pub(crate) fn around(
    area: Bounds<Pixels>,
    pages: impl IntoIterator<Item = Bounds<Pixels>>,
) -> Vec<Bounds<Pixels>> {
    let mut pieces = vec![area];
    for page in pages {
        pieces = pieces
            .into_iter()
            .flat_map(|piece| outside(piece, page))
            .collect();
    }
    pieces.retain(|piece| piece.size.width > px(0.) && piece.size.height > px(0.));
    pieces
}

/// `piece` without `page`: the bands above, below, left, and right of where
/// the two overlap, or `piece` itself where they do not.
fn outside(piece: Bounds<Pixels>, page: Bounds<Pixels>) -> Vec<Bounds<Pixels>> {
    let cut = piece.intersect(&page);
    if cut.size.width <= px(0.) || cut.size.height <= px(0.) {
        return vec![piece];
    }
    let mut bands = Vec::with_capacity(4);
    if cut.top() > piece.top() {
        bands.push(Bounds::from_corners(
            piece.origin,
            point(piece.right(), cut.top()),
        ));
    }
    if cut.bottom() < piece.bottom() {
        bands.push(Bounds::from_corners(
            point(piece.left(), cut.bottom()),
            point(piece.right(), piece.bottom()),
        ));
    }
    if cut.left() > piece.left() {
        bands.push(Bounds::from_corners(
            point(piece.left(), cut.top()),
            point(cut.left(), cut.bottom()),
        ));
    }
    if cut.right() < piece.right() {
        bands.push(Bounds::from_corners(
            point(cut.right(), cut.top()),
            point(piece.right(), cut.bottom()),
        ));
    }
    bands
}

/// The pages drawn this frame. Read while painting, after every page has
/// laid out.
fn drawn(pages: &Drawn) -> Vec<Bounds<Pixels>> {
    pages.borrow().values().copied().collect()
}

/// `color` over the whole of its parent, which must be `relative()`, except
/// where a page shows: for the window's root, which other children cover.
pub(crate) fn ground(color: Hsla, pages: Drawn) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            for piece in around(bounds, drawn(&pages)) {
                window.paint_quad(fill(piece, color));
            }
        },
    )
    .absolute()
    .inset_0()
}

/// `picture`, covering the whole of its parent as `ObjectFit::Cover` does,
/// except where a page shows. The parent must be `relative()`.
pub(crate) fn picture(picture: Arc<RenderImage>, pages: Drawn) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let fitted = ObjectFit::Cover.get_bounds(bounds, picture.size(0));
            for piece in around(bounds, drawn(&pages)) {
                if let Err(error) =
                    window.paint_image(piece, fitted, Corners::default(), picture.clone(), 0, false)
                {
                    tracing::debug!(%error, "Could not paint the backdrop picture");
                }
            }
        },
    )
    .absolute()
    .inset_0()
}

/// `child`, with `color` painted over its bounds first, as `.bg(color)` on it
/// would, except where a page shows. It takes the child's own layout, so the
/// child keeps its place among its siblings and its absolutely placed
/// descendants keep theirs.
pub(crate) struct AroundPages {
    child: AnyElement,
    color: Hsla,
    pages: Drawn,
}

impl AroundPages {
    pub(crate) fn new(child: impl IntoElement, color: Hsla, pages: Drawn) -> Self {
        Self {
            child: child.into_any_element(),
            color,
            pages,
        }
    }
}

impl IntoElement for AroundPages {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for AroundPages {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for piece in around(bounds, drawn(&self.pages)) {
            window.paint_quad(fill(piece, self.color));
        }
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests;
