use super::*;
use crate::control::{NotesTo, Target};

mod outcomes;
mod session_isolation;

fn recipient(view: &Entity<HerdrWindow>, cx: &VisualTestContext) -> crate::browser::FeedbackKey {
    view.read_with(cx, |view, _| crate::browser::FeedbackKey {
        scope: scope(&view.endpoints[0]),
        pane_id: "w0:p1".into(),
    })
}

/// A snapshot where pane `w0:p1` runs an idle agent.
fn with_idle_agent(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut shown: serde_json::Value =
                serde_json::to_value(view.live.snapshot.as_deref().unwrap()).unwrap();
            shown["panes"] = serde_json::json!([{
                "pane_id": "w0:p1", "workspace_id": "w0", "tab_id": "t0", "label": null,
                "cwd": null, "foreground_cwd": null, "focused": true,
                "right_click_passthrough": false
            }]);
            shown["agents"] = serde_json::json!([{
                "pane_id": "w0:p1", "workspace_id": "w0", "tab_id": "t0", "name": "claude",
                "display_agent": "Claude Code", "agent": "claude", "title": null,
                "terminal_title": null, "terminal_title_stripped": null,
                "agent_status": "idle", "state_change_seq": 0, "state_labels": [],
                "tokens": [], "focused": true
            }]);
            view.live.snapshot = Some(Arc::new(serde_json::from_value(shown).unwrap()));
        });
    });
}

fn deliver(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    daemon: Option<&str>,
    pane: &str,
) -> Option<NotesTo> {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let target = Target {
                daemon: daemon.map(std::path::Path::new),
                workspace: None,
                pane: Some(pane),
            };
            view.deliver_requested_notes(&target, "Picked: B\n", cx)
        })
    })
}

#[gpui::test]
fn requested_notes_go_only_to_a_pane_the_window_shows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_idle_agent(&view, cx);
    assert_eq!(deliver(&view, cx, None, "w0:p9"), None);
    // The same pane ID in another daemon's session is not this one.
    assert_eq!(
        deliver(&view, cx, Some("/elsewhere/herdr-client.sock"), "w0:p1"),
        None
    );
    view.read_with(cx, |view, _| assert_eq!(view.deliveries.len(), 0));
    let socket = view.read_with(cx, |view, _| {
        view.endpoints[0]
            .connection
            .target
            .socket_path()
            .ok()
            .map(|path| path.to_string_lossy().into_owned())
    });
    assert_eq!(
        deliver(&view, cx, socket.as_deref(), "w0:p1"),
        Some(NotesTo::Agent)
    );
    // Queued for the idle agent's prompt, the way page notes are.
    view.read_with(cx, |view, _| assert_eq!(view.deliveries.len(), 1));
}
