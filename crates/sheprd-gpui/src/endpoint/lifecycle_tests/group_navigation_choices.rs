use super::*;
use crate::browser::{Pick, Store};

enum Choice {
    OtherTerminal,
    FocusedTerminal,
    Page,
}

fn newer_choice(cx: &mut gpui::TestAppContext, choice: Choice) {
    let (endpoint, mut server) = connected_endpoint("newer-group-choice");
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
            let focused = shown.focused_tab_id.clone().unwrap();
            for id in ["newer", "replacement"] {
                let mut tab = shown.tabs[0].clone();
                tab.tab_id = id.into();
                tab.workspace_id = shown.focused_workspace_id.clone().unwrap();
                shown.tabs.push(tab);
            }
            view.live.snapshot = Some(Arc::new(shown.clone()));
            let (scope, workspace) = view.browser_key().unwrap();
            let moved = Store::update(cx, |store| {
                store.open(scope.clone(), &workspace, None, None)
            })
            .unwrap();
            view.show_browser_tab(moved, window, cx);
            let source = view.active_group().unwrap();
            view.split_group(source, window, cx);
            let destination = view.active_group().unwrap();
            view.live.surface = None;
            view.move_tab_to_group(Pick::Page(moved), destination, window, cx);
            assert_eq!(
                view.pending_navigation,
                Some(NavigationTarget::Tab("replacement".into()))
            );

            let expected = match choice {
                Choice::OtherTerminal => {
                    view.choose_herdr_tab(source, "newer", window, cx);
                    assert_eq!(
                        view.pending_navigation,
                        Some(NavigationTarget::Tab("newer".into()))
                    );
                    Pick::Herdr("newer".into())
                }
                Choice::FocusedTerminal => {
                    view.choose_herdr_tab(source, &focused, window, cx);
                    assert!(view.pending_navigation.is_none());
                    Pick::Herdr(focused)
                }
                Choice::Page => {
                    let page = Store::update(cx, |store| store.open(scope, &workspace, None, None))
                        .unwrap();
                    view.show_browser_tab_in(Some(source), page, window, cx);
                    assert!(view.pending_navigation.is_none());
                    Pick::Page(page)
                }
            };
            assert_eq!(view.active_group(), Some(source));
            assert_eq!(view.group_pick(source), Some(expected.clone()));
            {
                let mut state = view.endpoints[0].connection.inbox.lock().unwrap();
                state.apply(ClientEvent::Snapshot(Arc::new(shown.clone())));
                state.apply(ClientEvent::Surface(surface(&shown)));
            }
            project_until(view, cx, "surface after newer choice", |view| {
                view.pending_navigation.is_none()
                    && (view.live.activation.is_some() || view.input_ready())
            });
            assert_eq!(view.group_pick(source), Some(expected));
            // A wire sentinel proves that cancellation sends no old target.
            view.endpoints[0]
                .connection
                .handle
                .as_ref()
                .unwrap()
                .set_focus(&shown.boot_id, false)
                .unwrap();
        });
    });
    match choice {
        Choice::OtherTerminal => {
            let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
                panic!("missing navigation for the newer choice");
            };
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["method"], Method::TabFocus.as_str());
            assert_eq!(request["params"]["tab_id"], "newer");
        }
        Choice::FocusedTerminal | Choice::Page => {
            assert!(matches!(
                server.receive(),
                ClientMessage::ClientShellFocus { focused: false }
            ));
        }
    }
}

#[gpui::test]
fn another_terminal_replaces_the_deferred_page_move_target(cx: &mut gpui::TestAppContext) {
    newer_choice(cx, Choice::OtherTerminal);
}

#[gpui::test]
fn the_already_focused_terminal_cancels_the_deferred_page_move_target(
    cx: &mut gpui::TestAppContext,
) {
    newer_choice(cx, Choice::FocusedTerminal);
}

#[gpui::test]
fn a_page_choice_cancels_the_deferred_page_move_target(cx: &mut gpui::TestAppContext) {
    newer_choice(cx, Choice::Page);
}
