use super::*;
use crate::browser::GroupIds;

fn pick(id: &str) -> Pick {
    Pick::Herdr(id.into())
}

#[test]
fn spans_slide_from_where_they_started_to_where_they_head() {
    let from = Span {
        left: 10.,
        width: 100.,
    };
    let to = Span {
        left: 210.,
        width: 60.,
    };
    assert_eq!(from.lerp(to, 0.), from);
    assert_eq!(from.lerp(to, 1.), to);
    assert_eq!(
        from.lerp(to, 0.5),
        Span {
            left: 110.,
            width: 80.
        }
    );
    let now = Instant::now();
    assert_eq!(to.from(None, now), to);
    // A slide that started long ago has arrived.
    assert_eq!(to.from(Some((from, now)), now + SLIDE * 20), to);
    let midway = to.from(Some((from, now)), now + SLIDE / 2);
    assert!(midway.left > from.left && midway.left < to.left);
}

#[test]
fn the_first_choice_is_simply_there() {
    let mut bodies = TabBodies::default();
    let group = GroupIds::default().next();
    let frame = bodies.frame(group, Some(&pick("a")), Instant::now());
    assert!(frame.from.is_none());
}

#[test]
fn a_new_choice_slides_from_where_the_body_drew() {
    let mut bodies = TabBodies::default();
    let group = GroupIds::default().next();
    let now = Instant::now();
    let frame = bodies.frame(group, Some(&pick("a")), now);
    let drawn = Span {
        left: 12.,
        width: 140.,
    };
    frame.cells.drawn.set(Some(drawn));
    // The same choice next frame starts nothing; the strip places the body
    // afresh.
    let again = bodies.frame(group, Some(&pick("a")), now);
    assert!(again.from.is_none());
    assert_eq!(again.cells.drawn.get(), None);
    again.cells.drawn.set(Some(drawn));
    let moved = bodies.frame(group, Some(&pick("b")), now);
    assert_eq!(moved.from, Some((drawn, now)));
    assert!(moved.moving(now));
    assert!(!moved.moving(now + SLIDE * 20));
    // Once arrived the slide is forgotten.
    let later = bodies.frame(group, Some(&pick("b")), now + SLIDE * 20);
    assert!(later.from.is_none());
}

#[test]
fn a_choice_after_none_does_not_slide() {
    let mut bodies = TabBodies::default();
    let group = GroupIds::default().next();
    let now = Instant::now();
    let frame = bodies.frame(group, Some(&pick("a")), now);
    frame.cells.drawn.set(Some(Span::default()));
    bodies.frame(group, None, now);
    assert!(bodies.frame(group, Some(&pick("b")), now).from.is_none());
}

#[test]
fn closed_groups_forget_their_bodies() {
    let mut bodies = TabBodies::default();
    let mut ids = GroupIds::default();
    let (kept, gone) = (ids.next(), ids.next());
    bodies.frame(kept, Some(&pick("a")), Instant::now());
    bodies.frame(gone, Some(&pick("a")), Instant::now());
    bodies.retain(|group| group == kept);
    assert!(bodies.cells(kept).is_some());
    assert!(bodies.cells(gone).is_none());
}
