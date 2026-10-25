use super::*;
use std::time::Duration;

fn glide() -> Glide {
    Glide::new(&ListState::new(0, gpui::ListAlignment::Top, px(0.)))
}

fn frames(glide: &mut Glide, mut top: f32, end: f32, viewport: f32) -> (f32, usize) {
    let start = Instant::now();
    let mut count = 0;
    while let Some(distance) = glide.step(
        top,
        end,
        viewport,
        start + Duration::from_millis(16 * count as u64),
    ) {
        assert!(distance > 0., "only ever forward");
        top += distance;
        count += 1;
        assert!(count < 200, "reaches the end");
    }
    (top, count)
}

#[test]
fn a_few_new_lines_ease_in_over_several_frames() {
    let mut glide = glide();
    let start = Instant::now();
    let first = glide.step(0., 40., 600., start).unwrap();
    assert!(first > 0. && first < 40., "part of the way: {first}");
    let (top, count) = frames(&mut glide, first, 40., 600.);
    assert!(40. - top <= MIN_STEP, "at the end: {top}");
    assert!(count >= 5, "eases rather than jumps: {count} frames");
}

#[test]
fn far_behind_closes_most_of_the_way_at_once() {
    let mut glide = glide();
    let first = glide.step(0., 5000., 600., Instant::now()).unwrap();
    assert!(
        5000. - first <= 300.,
        "left at most half a viewport: {first}"
    );
}

#[test]
fn scrolling_up_stops_following_until_back_at_the_end() {
    let mut glide = glide();
    let now = Instant::now();
    let step = glide.step(0., 100., 600., now).unwrap();
    // The user scrolled up.
    glide.scrolled.set(true);
    assert_eq!(glide.step(step - 30., 100., 600., now), None);
    // New output does not pull the list down while the user reads.
    assert_eq!(glide.step(step - 30., 400., 600., now), None);
    // The user scrolled back to the end: it follows again.
    glide.scrolled.set(true);
    assert_eq!(glide.step(400., 400., 600., now), None);
    assert!(glide.step(400., 450., 600., now).is_some());
}

#[test]
fn the_list_moving_its_own_top_keeps_following() {
    let mut glide = glide();
    let now = Instant::now();
    let step = glide.step(0., 100., 600., now).unwrap();
    // The end came up short, say a line under the last message went away,
    // and the list pulled its top back up on its own.
    assert!(glide.step(step - 30., 120., 600., now).is_some());
}

#[test]
fn at_the_end_nothing_moves() {
    let mut glide = glide();
    assert_eq!(glide.step(100., 100., 600., Instant::now()), None);
    assert_eq!(glide.step(100., 100.3, 600., Instant::now()), None);
}

#[test]
fn a_stalled_frame_counts_as_a_short_one() {
    let mut glide = glide();
    let start = Instant::now();
    let first = glide.step(0., 200., 600., start).unwrap();
    let after_stall = glide
        .step(first, 200., 600., start + Duration::from_secs(2))
        .unwrap();
    assert!(first + after_stall < 200., "still easing: {after_stall}");
}

#[test]
fn redrawing_the_entry_at_the_top_keeps_the_place_in_it() {
    let list = ListState::new(3, gpui::ListAlignment::Top, px(0.));
    let inside = ListOffset {
        item_ix: 1,
        offset_in_item: px(20.),
    };
    list.scroll_to(inside);
    splice_in_place(&list, 1..2, 1);
    let top = list.logical_scroll_top();
    assert_eq!((top.item_ix, top.offset_in_item), (1, px(20.)));
    // The list's own splice moves the top to the entry's start.
    list.splice(1..2, 1);
    assert_eq!(list.logical_scroll_top().offset_in_item, px(0.));
    // Entries that come and go still move the top as the list does.
    list.scroll_to(inside);
    splice_in_place(&list, 0..1, 2);
    assert_eq!(list.logical_scroll_top().item_ix, 2);
}

#[test]
fn a_steady_stream_moves_steadily_rather_than_a_dash_per_chunk() {
    let mut glide = glide();
    let start = Instant::now();
    let (mut top, mut end) = (0., 0.);
    let mut steps = Vec::new();
    for frame in 0..120u64 {
        // A line of text every fifth frame, as a reply streams.
        if frame % 5 == 0 {
            end += 20.;
        }
        let now = start + Duration::from_millis(16 * frame);
        let step = glide.step(top, end, 600., now).unwrap_or(0.);
        top += step;
        steps.push(step);
    }
    // Once under way, no frame moves much more than the one before: the
    // speed carries from chunk to chunk.
    for pair in steps[30..].windows(2) {
        assert!(pair[1] - pair[0] < 2., "{pair:?}");
    }
    assert!(end - top < 60., "keeps up: {} behind", end - top);
}
