use super::*;
use herdr_client::protocol::{SemanticNotification, SemanticNotificationKind};

fn drawn(id: u64, corner: ToastHerdrPosition) -> Drawn {
    let notice = Notice::new(
        SemanticNotification {
            kind: SemanticNotificationKind::Finished,
            title: "Done".into(),
            body: None,
            sound: None,
            agent: None,
            workspace_id: None,
            tab_id: None,
            pane_id: None,
            position: None,
        },
        Instant::now(),
    );
    Drawn {
        key: Key {
            endpoint: "local".into(),
            generation: 1,
            id,
        },
        label: "Local".into(),
        inbox: Arc::default(),
        notice,
        corner,
    }
}

fn live() -> ToastMotion {
    ToastMotion {
        live: true,
        ..Default::default()
    }
}

#[test]
fn toasts_up_when_first_drawn_are_simply_there() {
    let start = Instant::now();
    let mut toasts = live();
    let first = drawn(1, ToastHerdrPosition::TopRight);
    toasts.observe(vec![first.clone()], start);
    assert_eq!(toasts.shown(&first.key, start), 1.);
    assert!(!toasts.moving());
}

#[test]
fn a_new_toast_slides_in_and_a_gone_one_slides_out() {
    let start = Instant::now();
    let mut toasts = live();
    let old = drawn(1, ToastHerdrPosition::TopRight);
    let new = drawn(2, ToastHerdrPosition::TopRight);
    toasts.observe(vec![old.clone()], start);
    toasts.observe(vec![new.clone()], start);
    assert_eq!(toasts.shown(&new.key, start), 0.);
    let leaving: Vec<_> = toasts.leaving(start).collect();
    assert_eq!(leaving.len(), 1);
    assert_eq!(leaving[0].0.drawn.key, old.key);
    assert_eq!(leaving[0].1, 1.);
    assert!(toasts.moving());
    let done = start + ENTER;
    toasts.observe(vec![new.clone()], done);
    assert_eq!(toasts.shown(&new.key, done), 1.);
    assert_eq!(toasts.leaving(done).count(), 0);
    assert!(!toasts.moving());
}

#[test]
fn a_toast_that_comes_back_stops_leaving() {
    let start = Instant::now();
    let mut toasts = live();
    let toast = drawn(1, ToastHerdrPosition::BottomLeft);
    toasts.observe(vec![toast.clone()], start);
    toasts.observe(Vec::new(), start);
    assert_eq!(toasts.leaving(start).count(), 1);
    toasts.observe(vec![toast.clone()], start);
    assert_eq!(toasts.leaving(start).count(), 0);
}

#[test]
fn unit_tests_see_toasts_settled() {
    let start = Instant::now();
    let mut toasts = ToastMotion::default();
    toasts.observe(Vec::new(), start);
    let toast = drawn(1, ToastHerdrPosition::TopLeft);
    toasts.observe(vec![toast.clone()], start);
    assert_eq!(toasts.shown(&toast.key, start), 1.);
    toasts.observe(Vec::new(), start);
    assert_eq!(toasts.leaving(start).count(), 0);
}

#[test]
fn toasts_slide_from_their_own_side() {
    assert_eq!(offset(ToastHerdrPosition::TopLeft, 0.), -SLIDE);
    assert_eq!(offset(ToastHerdrPosition::BottomRight, 0.), SLIDE);
    assert_eq!(offset(ToastHerdrPosition::TopRight, 1.), 0.);
}
