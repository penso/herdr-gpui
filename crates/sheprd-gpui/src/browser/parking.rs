//! Moving a hidden page out of the window's area.
//!
//! AppKit goes on finding a hidden view under the pointer during a drag. A
//! page's view stays where it last showed, above the older views there, so a
//! hidden page would stand in the way of every drag over the page showing
//! beneath it, or over a terminal drawn there: the page beneath sees its
//! drag start and end and nothing between. Giving up the hidden view's drag
//! types is not enough, since the drag then passes to the views that contain
//! it and never to the page beneath. So a hidden page is moved, at its own
//! size, to where no pointer reaches it.
use objc2_foundation::NSPoint;
use wry::WebViewExtMacOS as _;

/// Far outside any window, in the coordinates of the view that holds a page.
const AWAY: NSPoint = NSPoint::new(-100_000., -100_000.);

/// Where a hidden page was, which it goes back to as it shows.
pub(super) struct Parked(NSPoint);

/// Moves `view` out of the window's area. Call it once per hiding: a second
/// call would keep the place it was moved to.
pub(super) fn park(view: &wry::WebView) -> Parked {
    let view = view.webview();
    let place = view.frame().origin;
    view.setFrameOrigin(AWAY);
    Parked(place)
}

/// Moves `view` back to where it was before [`park`].
pub(super) fn unpark(view: &wry::WebView, Parked(place): Parked) {
    view.webview().setFrameOrigin(place);
}
