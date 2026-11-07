use super::*;

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), gpui::size(px(width), px(height)))
}

fn area(bounds: &Bounds<Pixels>) -> f32 {
    f32::from(bounds.size.width) * f32::from(bounds.size.height)
}

fn overlap(a: &Bounds<Pixels>, b: &Bounds<Pixels>) -> bool {
    let cut = a.intersect(b);
    cut.size.width > px(0.) && cut.size.height > px(0.)
}

/// The pieces cover all of `whole` but `pages`, once each.
fn covers_all_but(pieces: &[Bounds<Pixels>], whole: Bounds<Pixels>, pages: &[Bounds<Pixels>]) {
    let painted: f32 = pieces.iter().map(area).sum();
    let left_out: f32 = pages.iter().map(|page| area(&whole.intersect(page))).sum();
    assert!(
        (painted - (area(&whole) - left_out)).abs() < 0.01,
        "{pieces:?}"
    );
    for (index, piece) in pieces.iter().enumerate() {
        assert!(whole.contains(&piece.origin), "{piece:?}");
        for page in pages {
            assert!(!overlap(piece, page), "{piece:?} paints over {page:?}");
        }
        for other in &pieces[index + 1..] {
            assert!(!overlap(piece, other), "{piece:?} and {other:?}");
        }
    }
}

#[test]
fn with_no_page_the_whole_area_paints() {
    let whole = rect(0., 0., 800., 600.);
    assert_eq!(around(whole, []), vec![whole]);
}

#[test]
fn a_page_inside_leaves_four_bands_around_it() {
    let whole = rect(0., 0., 800., 600.);
    let page = rect(240., 72., 400., 400.);
    let pieces = around(whole, [page]);
    covers_all_but(&pieces, whole, &[page]);
    assert_eq!(pieces.len(), 4, "{pieces:?}");
}

#[test]
fn a_page_at_an_edge_leaves_no_band_beyond_it() {
    let whole = rect(0., 0., 800., 600.);
    let page = rect(240., 72., 560., 504.);
    let pieces = around(whole, [page]);
    covers_all_but(&pieces, whole, &[page]);
    // The page reaches the right edge: bands above, below, and left only.
    assert_eq!(pieces.len(), 3, "{pieces:?}");
}

#[test]
fn two_pages_side_by_side_both_stay_unpainted() {
    let whole = rect(0., 0., 1200., 800.);
    let left = rect(240., 72., 479.5, 700.);
    let right = rect(720.5, 72., 479.5, 700.);
    covers_all_but(&around(whole, [left, right]), whole, &[left, right]);
}

#[test]
fn a_page_elsewhere_changes_nothing() {
    let whole = rect(0., 0., 100., 100.);
    assert_eq!(around(whole, [rect(200., 0., 50., 50.)]), vec![whole]);
    // Touching an edge is not overlapping.
    assert_eq!(around(whole, [rect(100., 0., 50., 50.)]), vec![whole]);
}

#[test]
fn a_page_over_everything_leaves_nothing_to_paint() {
    let whole = rect(10., 10., 100., 100.);
    assert!(around(whole, [rect(0., 0., 200., 200.)]).is_empty());
}

#[test]
fn a_page_clipped_to_nothing_changes_nothing() {
    // An opening group's page before any of it shows.
    let whole = rect(0., 0., 100., 100.);
    let clipped = rect(50., 50., 0., 40.);
    assert_eq!(around(whole, [clipped]), vec![whole]);
}
