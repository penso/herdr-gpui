use super::*;

fn queue_for(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    target: Option<&str>,
    text: &str,
) {
    select(view, cx);
    write(view, cx, text);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.terminal_notes.composer.as_mut().unwrap().target = target.map(str::to_owned);
        });
    });
    cx.simulate_keystrokes("shift-enter");
}

fn clipboard(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap()
}

fn two_agents() -> ClientShellSnapshot {
    let mut snapshot = shown(Some("idle"), false);
    let mut second_pane = snapshot.panes[0].clone();
    second_pane.pane_id = "w0:p2".into();
    snapshot.panes.push(second_pane);
    let mut second_agent = snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == "w0:p1")
        .unwrap()
        .clone();
    second_agent.pane_id = "w0:p2".into();
    snapshot.agents.push(second_agent);
    snapshot
}

#[gpui::test]
fn unavailable_agents_and_shell_notes_all_survive_one_send(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, shown(None, false));
    queue_for(&view, cx, Some("missing-one"), "First agent.");
    queue_for(&view, cx, Some("missing-two"), "Second agent.");
    queue_for(&view, cx, None, "Shell note.");
    cx.update(|_, cx| view.update(cx, |view, cx| view.send_terminal_notes(cx)));
    let text = if cfg!(unix) {
        [
            kept(cx, "missing-one").unwrap(),
            kept(cx, "missing-two").unwrap(),
            clipboard(cx),
        ]
        .join("\n")
    } else {
        // Windows has no feedback collector: all three batches must be in
        // the single clipboard value, rather than only the final shell note.
        clipboard(cx)
    };
    for comment in ["First agent.", "Second agent.", "Shell note."] {
        assert_eq!(text.matches(comment).count(), 1, "{text}");
    }
    assert_eq!(
        view.read_with(cx, |view, _| view.terminal_notes.queued()),
        0
    );
}

#[gpui::test]
fn failed_pastes_for_two_agents_are_recovered_together(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, two_agents());
    queue_for(&view, cx, Some("w0:p1"), "First paste.");
    queue_for(&view, cx, Some("w0:p2"), "Second paste.");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.send_terminal_notes(cx);
            assert_eq!(view.deliveries.len(), 2);
            // Neither pane has a connection in this fixture.
            view.poll_deliveries(cx);
            assert_eq!(view.deliveries.len(), 0);
        });
    });
    let text = if cfg!(unix) {
        [kept(cx, "w0:p1").unwrap(), kept(cx, "w0:p2").unwrap()].join("\n")
    } else {
        clipboard(cx)
    };
    assert_eq!(text.matches("First paste.").count(), 1, "{text}");
    assert_eq!(text.matches("Second paste.").count(), 1, "{text}");
}

#[gpui::test]
fn later_paste_failures_preserve_the_sends_earlier_clipboard_notes(cx: &mut TestAppContext) {
    let mut snapshot = two_agents();
    snapshot
        .agents
        .iter_mut()
        .find(|agent| agent.pane_id == "w0:p2")
        .unwrap()
        .agent_status = AgentStatus::Working;
    let (view, cx) = window(cx, snapshot);
    queue_for(&view, cx, None, "Shell note.");
    queue_for(&view, cx, Some("w0:p1"), "First paste.");
    queue_for(&view, cx, Some("w0:p2"), "Late paste.");
    cx.update(|_, cx| view.update(cx, |view, cx| view.send_terminal_notes(cx)));
    assert!(clipboard(cx).contains("Shell note."));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.poll_deliveries(cx);
            assert_eq!(view.deliveries.len(), 1);
        })
    });
    if !cfg!(unix) {
        let text = clipboard(cx);
        assert!(text.contains("Shell note.") && text.contains("First paste."));
    }
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot
                .agents
                .iter_mut()
                .for_each(|agent| agent.agent_status = AgentStatus::Idle);
            view.live.snapshot = Some(Arc::new(snapshot));
            view.poll_deliveries(cx);
            assert_eq!(view.deliveries.len(), 0);
        })
    });
    let text = if cfg!(unix) {
        [
            clipboard(cx),
            kept(cx, "w0:p1").unwrap(),
            kept(cx, "w0:p2").unwrap(),
        ]
        .join("\n")
    } else {
        clipboard(cx)
    };
    for comment in ["Shell note.", "First paste.", "Late paste."] {
        assert_eq!(text.matches(comment).count(), 1, "{text}");
    }
}
