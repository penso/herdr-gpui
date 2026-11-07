use super::*;

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), gpui::size(px(width), px(height)))
}

#[test]
fn a_page_lands_on_whole_device_pixels() {
    // Half a point is a whole pixel on a Retina display, and stays.
    assert_eq!(
        snap(rect(10.5, 20.5, 100.5, 50.), 2.),
        rect(10.5, 20.5, 100.5, 50.)
    );
    // A quarter point is not: each edge goes to the nearest pixel, a tie
    // away from zero. The edges at 10.25 and 110.25 both move out.
    assert_eq!(
        snap(rect(10.25, 20.75, 100., 50.25), 2.),
        rect(10.5, 21., 100., 50.)
    );
    // At one pixel a point, edges land on whole points.
    assert_eq!(snap(rect(0.4, 0.6, 9.2, 9.), 1.), rect(0., 1., 10., 9.));
}

#[test]
fn a_scale_the_display_never_has_leaves_the_page_where_it_is() {
    let page = rect(0.3, 0.3, 10., 10.);
    for scale in [0., -2., f32::NAN, f32::INFINITY] {
        assert_eq!(snap(page, scale), page);
    }
}
