use super::*;
use crate::state::ConnectionStatus;

#[gpui::test]
fn detach_and_reconnect_commands_reset_connection_and_composition(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let epoch = view.selection_epoch;
            view.marked = "\u{b4}".into();
            view.marked_selection = Some(1..1);
            view.command(Command::Detach, window, cx);

            assert_eq!(view.live.status, ConnectionStatus::Detached);
            assert!(view.endpoints[0].connection.handle.is_none());
            assert_eq!(view.selection_epoch, epoch + 1);
            assert!(view.marked.is_empty());
            assert_eq!(view.marked_selection, None);

            view.marked = "\u{b4}".into();
            view.marked_selection = Some(1..1);
            // The fixture uses an explicit missing socket, never daemon discovery.
            view.command(Command::Reconnect, window, cx);

            assert_ne!(view.live.status, ConnectionStatus::Detached);
            assert!(view.endpoints[0].connection.handle.is_some());
            assert_eq!(view.selection_epoch, epoch + 2);
            assert!(view.marked.is_empty());
            assert_eq!(view.marked_selection, None);
        });
    });
}

#[gpui::test]
fn detach_default_chord_routes_to_the_local_command(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        window.focus(&view.read(cx).focus.clone(), cx);
        window.draw(cx).clear(cx);
        assert!(window.dispatch_keystroke(gpui::Keystroke::parse("ctrl-b").unwrap(), cx));
        assert!(window.dispatch_keystroke(gpui::Keystroke::parse("q").unwrap(), cx));
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.live.status, ConnectionStatus::Detached);
        assert!(view.endpoints[0].connection.handle.is_none());
    });
}
