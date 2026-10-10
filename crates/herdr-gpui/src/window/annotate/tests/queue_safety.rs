use super::*;
use crate::terminal_notes::MAX_NOTES;

fn copied(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap()
}

#[gpui::test]
fn reused_pane_ids_do_not_mix_notes_from_different_daemons(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(Some("working"), false));
    select(&view, cx);
    write(&view, cx, "Original session.");
    cx.simulate_keystrokes("shift-enter");

    // A session switch or restart reuses the endpoint slot and pane IDs.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot.boot_id = "replacement-boot".into();
            view.live.snapshot = Some(Arc::new(snapshot));
            let mut surface = (**view.live.surface.as_ref().unwrap()).clone();
            surface.boot_id = "replacement-boot".into();
            view.live.surface = Some(Arc::new(surface));
        });
    });
    select(&view, cx);
    write(&view, cx, "Current session.");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.terminal_notes.queued(), 0);
        assert_eq!(view.deliveries.len(), 1);
    });
    let text = copied(cx);
    assert!(text.contains("Original session."));
    assert!(!text.contains("Current session."));
    assert!(
        kept(&view, cx, "w0:p1").is_none(),
        "stale notes must not enter feedback"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot
                .agents
                .iter_mut()
                .for_each(|agent| agent.agent_status = AgentStatus::Idle);
            view.live.snapshot = Some(Arc::new(snapshot));
            view.poll_deliveries(cx);
        });
    });
    let sent = delivered(&view, cx, "w0:p1").unwrap();
    assert!(sent.contains("Current session."));
    if cfg!(unix) {
        assert!(
            !sent.contains("Original session."),
            "feedback belongs only to the current agent"
        );
    } else {
        // A failed Windows paste extends this Send's clipboard recovery,
        // preserving the stale notes that were already copied there.
        assert_eq!(sent.matches("Original session.").count(), 1);
        assert_eq!(sent.matches("Current session.").count(), 1);
        assert!(kept(&view, cx, "w0:p1").is_none());
    }
}

#[gpui::test]
fn replacing_an_endpoint_does_not_retarget_queued_notes(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(Some("working"), false));
    select(&view, cx);
    write(&view, cx, "Original device.");
    cx.simulate_keystrokes("shift-enter");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.endpoints[view.selected_endpoint].id = "another-device".into();
            view.send_terminal_notes(cx);
            assert_eq!(view.deliveries.len(), 0);
            assert_eq!(view.terminal_notes.queued(), 0);
        });
    });
    assert!(copied(cx).contains("Original device."));
    assert!(kept(&view, cx, "w0:p1").is_none());
}

#[gpui::test]
fn a_full_queue_can_be_reopened_and_sent_but_not_extended(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(Some("working"), false));
    for index in 0..MAX_NOTES {
        select(&view, cx);
        write(&view, cx, &format!("Queued note {index}."));
        cx.simulate_keystrokes("shift-enter");
    }
    assert!(!view.read_with(cx, |view, _| view.terminal_notes.composing()));
    select(&view, cx);
    write(&view, cx, "One too many.");
    for key in ["shift-enter", "enter"] {
        cx.simulate_keystrokes(key);
        view.read_with(cx, |view, cx| {
            assert_eq!(view.terminal_notes.queued(), MAX_NOTES);
            assert_eq!(view.deliveries.len(), 0);
            let composer = view.terminal_notes.composer.as_ref().unwrap();
            assert_eq!(composer.input.read(cx).text(), "One too many.");
        });
    }
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let input = view.terminal_notes.composer.as_ref().unwrap().input.clone();
            input.update(cx, |input, cx| input.set_text_selected("", cx));
        });
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.terminal_notes.queued(), 0);
        assert!(!view.terminal_notes.composing());
        assert_eq!(view.deliveries.len(), 1);
    });
}
