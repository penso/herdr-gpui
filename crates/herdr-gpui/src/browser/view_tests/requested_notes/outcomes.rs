use super::*;
use crate::browser::Feedback;

#[gpui::test]
fn a_pane_without_an_agent_reports_kept_notes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_idle_agent(&view, cx);
    let target = recipient(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap())
                .agents
                .clear();
        });
    });

    assert_eq!(deliver(&view, cx, None, "w0:p1"), Some(NotesTo::Kept));
    view.read_with(cx, |view, _| assert_eq!(view.deliveries.len(), 0));
    cx.update(|_, cx| {
        assert_eq!(
            cx.default_global::<Feedback>().take(&target).as_deref(),
            Some("Picked: B\n")
        );
    });
}

#[gpui::test]
fn a_waiter_receives_notes_without_a_detected_agent(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_idle_agent(&view, cx);
    let target = recipient(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap())
                .agents
                .clear();
        });
        cx.default_global::<Feedback>()
            .set_waiting(vec![target.clone()]);
    });

    assert_eq!(deliver(&view, cx, None, "w0:p1"), Some(NotesTo::Agent));
    view.read_with(cx, |view, _| assert_eq!(view.deliveries.len(), 0));
    cx.update(|_, cx| {
        assert_eq!(
            cx.default_global::<Feedback>().take(&target).as_deref(),
            Some("Picked: B\n")
        );
    });
}
