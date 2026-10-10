use super::*;

#[gpui::test]
fn sent_notes_cannot_enter_another_daemons_feedback(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(Some("working"), false));
    select(&view, cx);
    write(&view, cx, "Only for the original daemon.");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        cx.default_global::<crate::browser::Feedback>()
            .set_waiting(vec!["w0:p1".into()]);
        view.update(cx, |view, cx| {
            assert_eq!(view.deliveries.len(), 1);
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot.boot_id = "replacement-boot".into();
            view.live.snapshot = Some(Arc::new(snapshot));
            view.poll_deliveries(cx);
            assert_eq!(view.deliveries.len(), 0);
        });
    });
    assert!(
        kept(cx, "w0:p1").is_none(),
        "even a waiter with the same pane ID cannot receive stale notes"
    );
    let text = cx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap();
    assert!(text.contains("Only for the original daemon."));
}

#[gpui::test]
fn sent_notes_survive_endpoint_reordering_but_not_replacement(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(Some("working"), false));
    select(&view, cx);
    write(&view, cx, "Original device.");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let target = view.endpoints[view.selected_endpoint]
                .connection
                .target
                .clone();
            view.endpoints.insert(
                0,
                crate::endpoint::Endpoint::new("earlier".into(), "Earlier".into(), target, false),
            );
            view.selected_endpoint += 1;
            view.poll_deliveries(cx);
            assert_eq!(
                view.deliveries.len(),
                1,
                "reordering retains the pending batch"
            );
            view.endpoints[view.selected_endpoint].id = "replacement-device".into();
            view.poll_deliveries(cx);
            assert_eq!(view.deliveries.len(), 0);
        });
    });
    assert!(kept(cx, "w0:p1").is_none());
    let text = cx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap();
    assert!(text.contains("Original device."));
}
