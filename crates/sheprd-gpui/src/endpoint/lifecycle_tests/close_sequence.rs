use super::*;
use crate::browser::Pick;

#[gpui::test]
fn close_sequence_waits_for_the_previous_closes_ack_and_surface(cx: &mut gpui::TestAppContext) {
    for ack_first in [true, false] {
        let (endpoint, mut server) = connected_endpoint("close-sequence");
        let (fixture, cx) = cx.add_window_view(|window, cx| {
            Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
        });
        let view = fixture.read_with(cx, |fixture, _| fixture.0.clone());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.endpoints = vec![endpoint];
                view.options = ConnectOptions::default();
                view.reset_selected(cx);
                project_until(view, cx, "initial surface", SheprdWindow::input_ready);
                let shown = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                for id in ["first", "second"] {
                    let mut tab = shown.tabs[0].clone();
                    tab.tab_id = id.into();
                    tab.workspace_id = shown.focused_workspace_id.clone().unwrap();
                    tab.agent_status = AgentStatus::Idle;
                    shown.tabs.push(tab);
                }
                *view.endpoints[0].connection.inbox.lock().unwrap() = view.live.clone();
                view.close_tabs(
                    ["first", "second"].map(|id| Pick::Herdr(id.into())).into(),
                    window,
                    cx,
                );
                assert!(view.menu.page.is_none());
                assert!(!view.input_ready());
            });
        });
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing first close");
        };
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], Method::TabClose.as_str());
        assert_eq!(request["params"]["tab_id"], "first");
        server.respond(&request);
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing close surface barrier");
        };
        let barrier: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(barrier["method"], Method::ClientShellSurfaceSet.as_str());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let mut next = (**view.live.snapshot.as_ref().unwrap()).clone();
                next.revision += 1;
                next.tabs.retain(|tab| tab.tab_id != "first");
                view.endpoints[0]
                    .connection
                    .inbox
                    .lock()
                    .unwrap()
                    .apply(ClientEvent::Snapshot(Arc::new(next.clone())));
                project_until(view, cx, "closed tab snapshot", |view| {
                    view.live.snapshot.as_ref().unwrap().revision == next.revision
                });
                view.advance_close_run(window, cx);
                assert!(
                    view.menu.page.is_none(),
                    "a snapshot alone must not try the next close"
                );
                assert!(view.browser.closing.is_some());
                let ack = ClientEvent::Response {
                    request_id: barrier["id"].as_str().unwrap().into(),
                    response: serde_json::json!({"result": {
                        "type": "client_shell_surface_set", "active": true,
                        "projection_revision": next.revision
                    }}),
                };
                let frame = ClientEvent::Surface(surface(&next));
                let events = if ack_first {
                    [ack, frame]
                } else {
                    [frame, ack]
                };
                for (index, event) in events.into_iter().enumerate() {
                    view.endpoints[0]
                        .connection
                        .inbox
                        .lock()
                        .unwrap()
                        .apply(event);
                    project_until(view, cx, "close barrier progress", |view| {
                        if index == 1 {
                            view.input_ready()
                        } else if ack_first {
                            view.live.activation.as_ref().unwrap().revision == Some(next.revision)
                        } else {
                            view.live.surface.is_some()
                        }
                    });
                    view.advance_close_run(window, cx);
                    assert!(view.menu.page.is_none(), "no stale-connection dialog");
                    assert!(view.browser.closing.is_some());
                    if index == 0 {
                        assert_eq!(
                            view.live.activation.as_ref().unwrap().request,
                            barrier["id"].as_str().unwrap()
                        );
                    }
                }
            });
        });
        // Release the worker's in-flight barrier too: the model above drove
        // its acknowledgement independently to test both arrival orders.
        server.respond(&barrier);
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing automatic second close");
        };
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], Method::TabClose.as_str());
        assert_eq!(request["params"]["tab_id"], "second");
    }
}
