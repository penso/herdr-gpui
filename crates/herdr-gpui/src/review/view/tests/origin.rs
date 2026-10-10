use super::*;

#[gpui::test]
fn retained_review_notes_cannot_target_a_replacement_session(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, Some("working"));
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    note(&view, cx, line(3), "Original review.");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot.boot_id = "replacement-session".into();
            view.live.snapshot = Some(Arc::new(snapshot));
            view.send_review(the(view), cx);
            assert_eq!(view.deliveries.len(), 0);
        });
        let target = crate::browser::FeedbackKey {
            scope: crate::browser::scope(&view.read(cx).endpoints[0]),
            pane_id: "w0:p1".into(),
        };
        assert!(
            cx.default_global::<crate::browser::Feedback>()
                .take(&target)
                .is_none()
        );
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap();
        assert!(text.contains("Original review."));
    });
}
