use super::*;

#[gpui::test]
fn moving_the_selected_host_keeps_its_notes_deliverable(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(Some("working"), false));
    select(&view, cx);
    write(&view, cx, "Stay with this agent.");
    cx.simulate_keystrokes("shift-enter");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let target = view.endpoints[view.selected_endpoint]
                .connection
                .target
                .clone();
            view.endpoints.insert(
                0,
                crate::endpoint::Endpoint::new(
                    "earlier-device".into(),
                    "Earlier device".into(),
                    target,
                    false,
                ),
            );
            view.selected_endpoint += 1;
            view.send_terminal_notes(cx);
            assert_eq!(view.deliveries.len(), 1);
            assert_eq!(view.terminal_notes.queued(), 0);
        });
    });
    assert!(kept(cx, "w0:p1").is_none());
}

#[gpui::test]
fn shell_notes_from_multiple_hosts_share_one_clipboard_prompt(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    select(&view, cx);
    write(&view, cx, "First host.");
    cx.simulate_keystrokes("shift-enter");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.endpoints[view.selected_endpoint].id = "second-device".into();
        });
    });
    select(&view, cx);
    write(&view, cx, "Second host.");
    cx.simulate_keystrokes("enter");
    let text = cx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap();
    assert!(text.contains("1. On terminal text") && text.contains("2. On terminal text"));
    assert!(text.find("First host.").unwrap() < text.find("Second host.").unwrap());
    assert_eq!(
        view.read_with(cx, |view, _| view.terminal_notes.queued()),
        0
    );
}
