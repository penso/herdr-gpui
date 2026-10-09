use super::*;
use crate::terminal::Selection;
use serde_json::{Value, json};

const PLUGIN: &str = "plugin-annotate";
const SHELL: &str = "shell-build";

fn commands() -> Vec<ClientShellCommand> {
    let command = |id: &str, action, labels: &[&str], description: &str| ClientShellCommand {
        command_id: id.into(),
        description: Some(description.into()),
        action,
        binding_label: labels[0].into(),
        binding_labels: labels.iter().map(|label| (*label).into()).collect(),
    };
    vec![
        command(
            PLUGIN,
            ClientShellCommandAction::PluginAction,
            &["ctrl+alt+a", "prefix+y"],
            "Annotate selection",
        ),
        command(
            SHELL,
            ClientShellCommandAction::Shell,
            &["ctrl+alt+b"],
            "Build",
        ),
    ]
}

/// Two panes side by side, `w1:p1` focused at content revision 6, with a
/// plugin action and a shell command bound to keys.
fn prepare(view: &mut HerdrWindow, endpoint: Endpoint, cx: &mut Context<HerdrWindow>) {
    prepare_mouse(view, endpoint, cx);
    Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0].content_revision = 6;
    Arc::make_mut(view.live.snapshot.as_mut().unwrap()).commands = commands();
    let mut inbox = view.endpoints[1].connection.inbox.lock().unwrap();
    Arc::make_mut(inbox.snapshot.as_mut().unwrap()).commands = commands();
}

/// A pointer drag from the left half of one cell to the right half of
/// another, in surface grid cells; `release` ends the drag.
fn select(view: &mut HerdrWindow, from: (f32, f32), to: (f32, f32), release: bool) {
    let height = view.config.terminal.line_height();
    let surface = view.live.surface.as_deref().unwrap();
    let mut selection = Selection::begin(
        surface,
        from.0 * 10. + 2.,
        from.1 * height + 1.,
        10.,
        height,
        1,
    )
    .unwrap();
    selection.extend(surface, to.0 * 10. + 8., to.1 * height + 1., 10., height);
    if release {
        selection.release();
    }
    view.selection = Some(selection);
}

/// The parameters of the next `command.invoke`, skipping the surface fence
/// and anything else the window sends around it.
fn next_invoke(server: &mut Server) -> Value {
    loop {
        if let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() {
            let request: Value = serde_json::from_str(&request).unwrap();
            if request["method"] == Method::CommandInvoke.as_str() {
                return request["params"].clone();
            }
        }
    }
}

fn press(keys: &[&str], cx: &mut gpui::VisualTestContext) {
    for key in keys {
        cx.update(|window, cx| window.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), cx));
    }
}

/// Grid cells (3, 2) to (7, 3) of the focused pane, whose text starts at
/// grid (1, 1): pane-relative columns, absolute buffer rows, both ends
/// inclusive, fenced by the revision the highlight was painted from.
fn painted_selection() -> Value {
    json!({
        "pane_id": "w1:p1",
        "anchor": {"row": 1, "col": 2},
        "cursor": {"row": 2, "col": 6},
        "content_revision": 6,
    })
}

fn invocation(id: &str, selection: Option<Value>) -> Value {
    let focused = snapshot();
    let mut params = json!({
        "command_id": id,
        "workspace_id": focused.focused_workspace_id,
        "tab_id": focused.focused_tab_id,
        "pane_id": focused.focused_pane_id,
    });
    if let Some(selection) = selection {
        params["selection"] = selection;
    }
    params
}

#[derive(Clone, Copy, Debug)]
enum Case {
    DirectKey,
    PrefixChord,
    ShellCommand,
    NoSelection,
    OtherPane,
    StillDragging,
    PopupOpen,
}

/// A plugin action, by direct key or prefix chord, carries the focused pane's
/// retained highlight so Herdr can hand the plugin its text; shell commands,
/// highlights elsewhere, drags in progress, and panes under a popup do not.
/// The keys that invoke the action leave the highlight in place.
#[gpui::test]
fn plugin_actions_carry_the_focused_panes_selection(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for case in [
        Case::DirectKey,
        Case::PrefixChord,
        Case::ShellCommand,
        Case::NoSelection,
        Case::OtherPane,
        Case::StillDragging,
        Case::PopupOpen,
    ] {
        let (endpoint, mut server) = connected_endpoint("plugin-selection");
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                prepare(view, endpoint, cx);
                match case {
                    Case::NoSelection => {}
                    Case::OtherPane => select(view, (43., 2.), (47., 3.), true),
                    Case::StillDragging => select(view, (3., 2.), (7., 3.), false),
                    _ => select(view, (3., 2.), (7., 3.), true),
                }
                if let Case::PopupOpen = case {
                    image_popup(view, "popup-1");
                }
            })
        });
        let (keys, expected): (&[&str], _) = match case {
            Case::DirectKey => (
                &["ctrl-alt-a"],
                invocation(PLUGIN, Some(painted_selection())),
            ),
            Case::PrefixChord => (
                &["ctrl-b", "y"],
                invocation(PLUGIN, Some(painted_selection())),
            ),
            Case::ShellCommand => (&["ctrl-alt-b"], invocation(SHELL, None)),
            _ => (&["ctrl-alt-a"], invocation(PLUGIN, None)),
        };
        press(keys, cx);
        assert_eq!(next_invoke(&mut server), expected, "{case:?}");
        let kept = view.read_with(cx, |view, _| view.selection.is_some());
        assert_eq!(kept, !matches!(case, Case::NoSelection), "{case:?}");
    }
}

/// The highlight is the session's: a restarted daemon or a reconnect never
/// sees coordinates painted from the connection before it.
#[gpui::test]
fn a_selection_never_outlives_its_session(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, _server) = connected_endpoint("plugin-selection");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            prepare(view, endpoint, cx);
            select(view, (3., 2.), (7., 3.), true);
            assert!(view.plugin_selection().is_some());
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.boot_id.push_str("-restarted");
            assert_eq!(view.plugin_selection(), None);
            view.reset_selected(cx);
            assert!(view.selection.is_none());
            assert_eq!(view.plugin_selection(), None);
        })
    });
}

/// A copy-mode mark is a selection too, and a mark left on a connection that
/// was replaced is not sent to the new one.
#[gpui::test]
fn a_copy_mode_mark_is_the_selection_of_its_connection(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("plugin-selection");
    let key = |key: &str| gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse(key).unwrap(),
        is_held: false,
        prefer_character_input: false,
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(view, endpoint, cx);
            view.live.supports_copy_motion = true;
            view.enter_copy_mode(window, cx);
            // No visible cursor: copy mode starts on the last row's first cell.
            for typed in ["v", "l", "l"] {
                view.copy_mode_key(&key(typed), cx);
            }
        })
    });
    press(&["ctrl-alt-a"], cx);
    let marked = json!({
        "pane_id": "w1:p1",
        "anchor": {"row": 21, "col": 0},
        "cursor": {"row": 21, "col": 2},
        "content_revision": 6,
    });
    assert_eq!(next_invoke(&mut server), invocation(PLUGIN, Some(marked)));
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert!(view.copy_mode_active());
            view.endpoints[1].connection.scrollback = Default::default();
            assert_eq!(view.plugin_selection(), None);
        })
    });
}

/// The palette captures the highlight with its target when it opens, so what
/// its search field does to focus afterwards cannot change which cells are
/// sent, and fences them by the frame on screen when the action runs.
#[gpui::test]
fn the_palette_sends_the_selection_captured_when_it_opened(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("plugin-selection");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(view, endpoint, cx);
            select(view, (3., 2.), (7., 3.), true);
            view.open_palette(crate::palette::Filter::Commands, window, cx);
            view.selection = None;
            view.filter_palette("Annotate selection", cx);
            // Output while the palette is open moves the pane on; the action
            // is fenced by the frame on screen when it runs.
            Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0].content_revision = 8;
        })
    });
    // Ranking runs on the background executor.
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let enter = gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("enter").unwrap(),
                is_held: false,
                prefer_character_input: false,
            };
            view.palette_key(&enter, window, cx);
        })
    });
    let mut selection = painted_selection();
    selection["content_revision"] = json!(8);
    assert_eq!(
        next_invoke(&mut server),
        invocation(PLUGIN, Some(selection))
    );
}
