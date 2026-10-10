use super::*;
use crate::browser::{Feedback, Scope};

#[gpui::test]
fn another_sessions_waiter_cannot_intercept_a_pane_delivery(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_idle_agent(&view, cx);
    let target = recipient(&view, cx);
    let mut other = target.clone();
    other.scope = Scope::local(std::path::Path::new("/other-session.sock"));
    cx.update(|_, cx| {
        cx.default_global::<Feedback>()
            .set_waiting(vec![other.clone()])
    });
    assert_eq!(deliver(&view, cx, None, "w0:p1"), Some(NotesTo::Agent));
    view.read_with(cx, |view, _| assert_eq!(view.deliveries.len(), 1));
    // The fixture has no transport. A failed paste must retain its scope too.
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_deliveries(cx)));
    cx.update(|_, cx| {
        assert_eq!(cx.default_global::<Feedback>().take(&other), None);
        assert_eq!(
            cx.default_global::<Feedback>().take(&target).as_deref(),
            Some("Picked: B\n")
        );
    });
}
