use super::*;
use crate::window::prefix::KeyChars;

/// Connects a new host to `view` whose custom command `command-v1` is bound
/// to `binding`, under the keymap `keys` and `pane_keys`, with the key's
/// Shift and layout read as AppKit's current event would give them for
/// Shift+1 on a U.S. layout.
fn connect(
    view: &gpui::Entity<SheprdWindow>,
    binding: &str,
    keys: &str,
    pane_keys: &[(&str, &str)],
    cx: &mut gpui::VisualTestContext,
) -> Server {
    let (endpoint, server) = connected_endpoint("ssh:fixture");
    let keys: toml::Table = keys.parse().unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.truncate(1);
            view.endpoints.push(endpoint);
            view.selected_endpoint = 1;
            view.options = ConnectOptions::default();
            view.reset_selected(cx);
            view.activation_deadline = None;
            let relabel = |snapshot: &mut Arc<ClientShellSnapshot>| {
                Arc::make_mut(snapshot).commands[0].binding_labels = vec![binding.into()];
            };
            relabel(view.live.snapshot.as_mut().unwrap());
            let mut inbox = view.endpoints[1].connection.inbox.lock().unwrap();
            relabel(inbox.snapshot.as_mut().unwrap());
            drop(inbox);
            assert!(view.input_ready());
            let pane_keys = pane_keys
                .iter()
                .map(|(typed, sent)| ((*typed).to_owned(), (*sent).to_owned()))
                .collect();
            view.config.keybindings = crate::keymap::Keymap::with_overrides(
                &Default::default(),
                &pane_keys,
                &crate::keymap::DaemonKeys::from_table(Some(&keys)),
            )
            .unwrap();
            view.key_press = || Some((18, true));
            view.key_layout = |_| Some(KeyChars::typing(["1", "!", "1", "1"], false));
        })
    });
    server
}

fn window(
    cx: &mut gpui::TestAppContext,
) -> (gpui::Entity<SheprdWindow>, &mut gpui::VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    (view, cx)
}

fn press(keys: &[&str], cx: &mut gpui::VisualTestContext) {
    for key in keys {
        cx.update(|window, cx| window.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), cx));
    }
}

#[track_caller]
fn assert_invoked(server: &mut Server, binding: &str) {
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("{binding}: missing command");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(
        request["method"],
        Method::CommandInvoke.as_str(),
        "{binding}"
    );
    assert_eq!(request["params"]["command_id"], "command-v1", "{binding}");
}

/// Shift+1 as GPUI's macOS backend delivers it, `!` without Shift, invokes a
/// daemon custom command bound to `prefix+shift+1` or `ctrl+alt+shift+1`.
/// Not `ctrl+shift+1`, which Linux's default keymap holds for tab 1.
#[gpui::test]
fn a_shifted_key_invokes_a_custom_command_through_the_window(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    for (binding, keys) in [
        ("prefix+shift+1", &["ctrl-b", "!"][..]),
        ("ctrl+alt+shift+1", &["ctrl-alt-!"][..]),
    ] {
        let mut server = connect(&view, binding, "", &[], cx);
        press(keys, cx);
        assert_invoked(&mut server, binding);
    }
}

/// A custom command bound to the symbol itself wins over a keymap chord on
/// the restored key, as it did before Shift was restored.
#[gpui::test]
fn a_custom_command_on_the_symbol_wins_over_a_shifted_chord(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let mut server = connect(
        &view,
        "prefix+!",
        "toggle_sidebar = 'prefix+shift+1'",
        &[],
        cx,
    );
    let visible = view.read_with(cx, |view, _| view.sidebar_visible);
    press(&["ctrl-b", "!"], cx);
    assert_invoked(&mut server, "prefix+!");
    assert_eq!(
        view.read_with(cx, |view, _| view.sidebar_visible),
        visible,
        "the chord did not run"
    );
}

/// Without the prefix, a key the keymap or a pane key holds as GPUI named it
/// keeps that meaning: `ctrl-alt-!` is left to the sidebar binding or the
/// pane key, not taken by a custom command bound to `ctrl+alt+shift+1`.
#[gpui::test]
fn a_key_held_as_named_is_not_taken_by_a_restored_custom_command(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    for (keys, pane_keys) in [
        ("toggle_sidebar = 'ctrl+alt+!'", &[][..]),
        ("", &[("ctrl-alt-!", "text:x")][..]),
    ] {
        let _server = connect(&view, "ctrl+alt+shift+1", keys, pane_keys, cx);
        let claimed = cx.update(|window, cx| {
            window.dispatch_keystroke(gpui::Keystroke::parse("ctrl-alt-!").unwrap(), cx)
        });
        assert!(
            !claimed,
            "{keys} {pane_keys:?}: a custom command took the key"
        );
    }
    // Held by nothing, the same key runs the custom command.
    let mut server = connect(&view, "ctrl+alt+shift+1", "", &[], cx);
    press(&["ctrl-alt-!"], cx);
    assert_invoked(&mut server, "ctrl+alt+shift+1");
}

/// Without the prefix, a custom command on the symbol runs although the
/// restored key is a second prefix.
#[gpui::test]
fn a_custom_command_on_the_symbol_wins_over_a_restored_prefix(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let mut server = connect(
        &view,
        "ctrl+alt+!",
        "prefix = ['ctrl+b', 'ctrl+alt+shift+1']",
        &[],
        cx,
    );
    press(&["ctrl-alt-!"], cx);
    assert_invoked(&mut server, "ctrl+alt+!");
    assert!(
        !view.read_with(cx, |view, _| view.prefix_armed),
        "the prefix armed"
    );
}
