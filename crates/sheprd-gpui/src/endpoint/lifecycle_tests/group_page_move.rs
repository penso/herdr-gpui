use super::*;
use crate::browser::{Pick, Store};

#[gpui::test]
fn moving_a_page_steers_its_source_terminal_without_taking_focus(cx: &mut gpui::TestAppContext) {
    for ready in [true, false] {
        let (endpoint, mut server) = connected_endpoint("page-move");
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
                let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
                let mut next = shown.tabs[0].clone();
                next.tab_id = "next".into();
                next.workspace_id = shown.focused_workspace_id.clone().unwrap();
                shown.tabs.push(next);
                view.live.snapshot = Some(Arc::new(shown.clone()));
                let (scope, workspace) = view.browser_key().unwrap();
                let page =
                    Store::update(cx, |store| store.open(scope, &workspace, None, None)).unwrap();
                view.show_browser_tab(page, window, cx);
                let source = view.active_group().unwrap();
                view.split_group(source, window, cx);
                let destination = view.active_group().unwrap();
                assert_eq!(view.primary_group(), Some(source));
                if !ready {
                    view.live.surface = None;
                }
                view.move_tab_to_group(Pick::Page(page), destination, window, cx);
                assert_eq!(view.group_pick(source), Some(Pick::Herdr("next".into())));
                assert_eq!(view.active_group(), Some(destination));
                assert_eq!(view.group_pick(destination), Some(Pick::Page(page)));
                assert!(
                    view.group_address(destination, cx)
                        .read(cx)
                        .focus
                        .is_focused(window)
                );
                if !ready {
                    assert_eq!(
                        view.pending_navigation,
                        Some(NavigationTarget::Tab("next".into()))
                    );
                    // A coherent projection resumes the deferred navigation.
                    let mut state = view.endpoints[0].connection.inbox.lock().unwrap();
                    state.apply(ClientEvent::Snapshot(Arc::new(shown.clone())));
                    state.apply(ClientEvent::Surface(surface(&shown)));
                    drop(state);
                    project_until(view, cx, "deferred tab focus", |view| {
                        view.pending_navigation.is_none()
                    });
                }
                assert_eq!(
                    view.live
                        .activation
                        .as_ref()
                        .and_then(|activation| activation.focus.as_ref()),
                    Some(&NavigationTarget::Tab("next".into()))
                );
                assert!(!view.input_ready(), "the replacement tab must be fenced");
            });
        });
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing navigation to the source group's replacement tab");
        };
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], Method::TabFocus.as_str());
        assert_eq!(request["params"]["tab_id"], "next");
    }
}
