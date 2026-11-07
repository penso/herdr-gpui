use super::*;

#[test]
fn where_gpui_saw_the_pointer_its_hit_test_decides() {
    let at = point(px(200.), px(150.));
    let over_page = Seen {
        at: Some(at),
        reached: true,
    };
    let over_toast = Seen {
        at: Some(at),
        reached: false,
    };
    assert!(routes(over_page, at));
    assert!(!routes(over_toast, at));
    // Less than half a point away is the same spot.
    assert!(!routes(over_toast, point(px(200.4), px(149.6))));
}

#[test]
fn elsewhere_the_page_takes_the_event() {
    // GPUI never saw the pointer, or saw it elsewhere, as after a drag
    // inside the page: nothing of the window's is known to lie there.
    let seen = Seen {
        at: Some(point(px(200.), px(150.))),
        reached: false,
    };
    assert!(routes(seen, point(px(240.), px(150.))));
    assert!(routes(Seen::default(), point(px(0.), px(0.))));
}

#[test]
fn a_page_hears_the_pointer_leave_as_it_goes_under_the_window() {
    let over_page = Seen {
        at: Some(point(px(200.), px(150.))),
        reached: true,
    };
    let under = Seen {
        reached: false,
        ..over_page
    };
    // From the page onto a divider's grip or a toast over it.
    assert!(went_under(over_page, false, true));
    // Out of the page's area, where AppKit tells the page itself.
    assert!(!went_under(over_page, false, false));
    // Still over the page, already told, or back onto the page.
    assert!(!went_under(over_page, true, true));
    assert!(!went_under(under, false, true));
    assert!(!went_under(under, true, true));
    // Before GPUI saw the pointer, the page heard nothing to undo.
    assert!(!went_under(Seen::default(), false, true));
}

#[test]
fn a_page_measured_from_the_top_lies_where_appkit_measures_from_the_bottom() {
    let page = Bounds::new(point(px(10.), px(20.)), gpui::size(px(100.), px(50.)));
    // A window 300 points tall: the page's top is 20 points down, so its
    // bottom is 300 - 20 - 50 = 230 points up.
    assert_eq!(
        frame(page, 300., false),
        NSRect::new(NSPoint::new(10., 230.), NSSize::new(100., 50.))
    );
    assert_eq!(
        frame(page, 300., true),
        NSRect::new(NSPoint::new(10., 20.), NSSize::new(100., 50.))
    );
}

#[test]
fn appkit_points_become_the_window_points_gpui_uses() {
    assert_eq!(
        window_point(NSPoint::new(10., 230.), 300., false),
        point(px(10.), px(70.))
    );
    assert_eq!(
        window_point(NSPoint::new(10., 230.), 300., true),
        point(px(10.), px(230.))
    );
    // A frame and a point inside it agree on where the point lies.
    let page = Bounds::new(point(px(10.), px(20.)), gpui::size(px(100.), px(50.)));
    let rect = frame(page, 300., false);
    let inside = NSPoint::new(rect.origin.x + 1., rect.origin.y + 1.);
    assert!(page.contains(&window_point(inside, 300., false)));
}
