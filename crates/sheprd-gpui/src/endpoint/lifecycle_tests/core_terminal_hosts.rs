//! herdr-core runs on this computer, so a workspace on another host never
//! gets its terminals from it, whatever `herdr_2_terminals` says.

use super::*;
use crate::core_terminals::State;

fn ssh() -> ConnectTarget {
    ConnectTarget::Ssh {
        target: "fixture".into(),
        session: "default".into(),
    }
}

#[gpui::test]
fn only_a_tab_on_this_computer_follows_the_herdr_2_setting(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let (mut remote, _server) = connected_endpoint("ssh:fixture");
    remote.connection.target = ssh();
    view.update(cx, |view, cx| {
        view.endpoints.push(remote);
        for (selected, setting, core) in [
            (0, true, true),
            (0, false, false),
            (1, true, false),
            (1, false, false),
        ] {
            view.selected_endpoint = selected;
            view.reset_selected(cx);
            view.config.herdr_2_terminals = setting;
            assert_eq!(view.new_tabs_use_core(), core, "{selected} {setting}");
        }
    });
}

#[gpui::test]
fn a_new_tab_on_an_ssh_host_is_created_by_its_herdr(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, mut server) = connected_endpoint("ssh:fixture");
    endpoint.connection.target = ssh();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(endpoint);
            view.selected_endpoint = 1;
            view.options = ConnectOptions::default();
            view.reset_selected(cx);
            view.activation_deadline = None;
            view.config.herdr_2_terminals = true;
            view.settings.shared =
                Some(crate::herdr_settings::Settings::parse_text(NO_TAB_PROMPT).unwrap());
            view.command(Command::Tab, window, cx);
            assert!(matches!(view.core_terminals.hub.state(), State::Idle));
        })
    });
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing creation");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], Method::TabCreate.as_str());
    assert_eq!(
        request["params"],
        serde_json::json!({"workspace_id": "w1", "focus": true})
    );
}

#[gpui::test]
fn the_herdr_2_command_is_refused_on_an_ssh_host(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, _server) = connected_endpoint("ssh:fixture");
    endpoint.connection.target = ssh();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(endpoint);
            view.selected_endpoint = 1;
            view.reset_selected(cx);
            view.command(Command::NewCoreTerminal, window, cx);
            assert!(matches!(view.core_terminals.hub.state(), State::Idle));
            let (flash, _) = view.flash.as_ref().unwrap();
            assert_eq!(flash.text, crate::i18n::fl!("core-terminals-local-only"));
        })
    });
}

/// Creating at once, rather than asking for the tab's name first.
const NO_TAB_PROMPT: &str = "[ui]\nprompt_new_tab_name = false\n";
