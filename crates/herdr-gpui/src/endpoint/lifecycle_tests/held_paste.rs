//! A paste or a file drop during a snapshot/surface gap waits with the keys
//! typed around it and arrives, in the user's order, once the gap closes.
use super::*;
use gpui::{ClipboardEntry, EntityInputHandler, ExternalPaths};

const STOPPED: &str = "Input not sent: the terminal stopped updating";

fn held_window(
    cx: &mut gpui::TestAppContext,
) -> (
    Server,
    gpui::Entity<HerdrWindow>,
    &mut gpui::VisualTestContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, server) = connected_endpoint("held");
    view.update(cx, |view, cx| prepare_mouse(view, endpoint, cx));
    (server, view, cx)
}

/// Publish a newer snapshot focused on `focus` without its surface, and
/// return the surface that closes the gap.
///
/// The snapshot also republishes the host's key bindings: a snapshot that
/// changes only metadata keeps input flowing, so a gap that holds input
/// must change something input depends on even when focus stays put.
fn open_gap(
    view: &mut HerdrWindow,
    cx: &mut Context<HerdrWindow>,
    focus: &str,
) -> Arc<PaneSurfaceFrame> {
    let mut next = (**view.live.snapshot.as_ref().unwrap()).clone();
    next.revision += 1;
    next.focused_pane_id = Some(focus.into());
    next.server_keybindings_toml = Some(format!("[keys]\n# revision {}", next.revision));
    let mut frame = (**view.live.surface.as_ref().unwrap()).clone();
    frame.projection_revision = next.revision;
    view.endpoints[1]
        .connection
        .inbox
        .lock()
        .unwrap()
        .apply(ClientEvent::Snapshot(Arc::new(next)));
    project_until(view, cx, "newer snapshot", |view| !view.input_ready());
    Arc::new(frame)
}

fn close_gap(view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>, frame: Arc<PaneSurfaceFrame>) {
    view.endpoints[1]
        .connection
        .inbox
        .lock()
        .unwrap()
        .apply(ClientEvent::Surface(frame));
    project_until(view, cx, "matching surface", HerdrWindow::input_ready);
}

fn pane_input(pane: &str, event: ClientPaneInputEvent) -> ClientMessage {
    ClientMessage::ClientShellPaneInput {
        pane_id: pane.into(),
        events: vec![event],
    }
}

fn text(text: &str) -> ClientPaneInputEvent {
    ClientPaneInputEvent::TextCommit(text.into())
}

fn paste(text: &str) -> ClientPaneInputEvent {
    ClientPaneInputEvent::Paste(text.into())
}

#[gpui::test]
fn a_paste_in_a_gap_arrives_between_the_keys_typed_around_it(cx: &mut gpui::TestAppContext) {
    let (mut server, view, cx) = held_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
            let frame = open_gap(view, cx, "w1:p1");
            view.replace_text_in_range(None, "a", window, cx);
            view.paste(cx);
            view.replace_text_in_range(None, "b", window, cx);
            assert_eq!(
                view.pending_input.len(),
                3,
                "nothing leaves before the gap closes"
            );
            close_gap(view, cx, frame);
            // Background clipboard acquisition holds `b` until the paste is ready.
            let waiting = usize::from(cfg!(any(target_os = "macos", target_os = "linux")));
            assert_eq!(view.pending_input.len(), waiting);
            view.send(text("sentinel"), cx);
        });
    });
    cx.run_until_parked();
    for expected in [text("a"), paste("pasted"), text("b"), text("sentinel")] {
        assert_eq!(server.receive(), pane_input("w1:p1", expected));
    }
    wait_image_finished(&view, cx);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui::test]
fn copied_files_pasted_in_a_gap_arrive_as_paths(cx: &mut gpui::TestAppContext) {
    let (mut server, view, cx) = held_window(cx);
    view.update(cx, |view, cx| {
        let paths = [
            PathBuf::from("/tmp/held-a.txt"),
            PathBuf::from("/tmp/held b.txt"),
        ];
        let mut entries = vec![ClipboardEntry::ExternalPaths(ExternalPaths(
            paths.iter().cloned().collect(),
        ))];
        entries.extend(ClipboardItem::new_string("held-a.txt".into()).into_entries());
        cx.write_to_clipboard(ClipboardItem { entries });
        let frame = open_gap(view, cx, "w1:p1");
        view.paste(cx);
        assert_eq!(view.pending_input.len(), 1);
        close_gap(view, cx, frame);
        view.send(text("sentinel"), cx);
    });
    cx.run_until_parked();
    for expected in [paste("/tmp/held-a.txt /tmp/held\\ b.txt"), text("sentinel")] {
        assert_eq!(server.receive(), pane_input("w1:p1", expected));
    }
    wait_image_finished(&view, cx);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui::test]
fn a_paste_held_in_a_gap_is_not_sent_when_focus_moves(cx: &mut gpui::TestAppContext) {
    let (mut server, view, cx) = held_window(cx);
    view.update(cx, |view, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
        let frame = open_gap(view, cx, "w1:p2");
        view.paste(cx);
        assert_eq!(view.pending_input.len(), 1);
        close_gap(view, cx, frame);
        view.send(text("sentinel"), cx);
    });
    cx.run_until_parked();
    assert_eq!(server.receive(), pane_input("w1:p2", text("sentinel")));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.local_error.as_deref(),
            Some("Input not sent: focus changed before it arrived")
        );
    });
}

#[gpui::test]
fn a_paste_held_in_a_gap_is_dropped_on_reset(cx: &mut gpui::TestAppContext) {
    let (mut server, view, cx) = held_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
            let frame = open_gap(view, cx, "w1:p1");
            view.replace_text_in_range(None, "a", window, cx);
            view.paste(cx);
            assert_eq!(view.pending_input.len(), 2);
            view.reset_selected(cx);
            assert_eq!(view.pending_input.len(), 0);
            close_gap(view, cx, frame);
            view.send(text("sentinel"), cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(server.receive(), pane_input("w1:p1", text("sentinel")));
}

#[gpui::test]
fn a_fifth_held_paste_is_refused_and_the_first_four_arrive(cx: &mut gpui::TestAppContext) {
    let (mut server, view, cx) = held_window(cx);
    view.update(cx, |view, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
        let frame = open_gap(view, cx, "w1:p1");
        for _ in 0..4 {
            view.paste(cx);
        }
        assert!(view.local_error.is_none());
        view.paste(cx);
        // The four pastes, and the mark where the fifth was refused.
        assert_eq!(view.pending_input.len(), 5);
        assert_eq!(view.local_error.as_deref(), Some(STOPPED));
        view.local_error = None;
        close_gap(view, cx, frame);
        view.send(text("sentinel"), cx);
    });
    cx.run_until_parked();
    for expected in [
        paste("pasted"),
        paste("pasted"),
        paste("pasted"),
        paste("pasted"),
        text("sentinel"),
    ] {
        assert_eq!(server.receive(), pane_input("w1:p1", expected));
    }
    wait_image_finished(&view, cx);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

/// Drop a file with the unfocused pane `w1:p2` under the pointer, or where it
/// was in the last frame.
fn drop_on_second_pane(
    view: &mut HerdrWindow,
    window: &mut gpui::Window,
    cx: &mut Context<HerdrWindow>,
) {
    let line = view.config.terminal.line_height();
    view.bounds.origin = window.mouse_position() - point(px(50.5 * 10.), px(5.5 * line));
    let dropped = ExternalPaths(std::iter::once(PathBuf::from("/tmp/held drop.txt")).collect());
    view.drop_terminal_files(&dropped, window, cx);
    assert_eq!(view.pending_input.len(), 1, "the drop is held");
}

/// The drop is made on a frame another client sized: the one the user sees.
#[gpui::test]
fn a_drop_in_a_gap_reaches_the_pane_it_was_dropped_on(cx: &mut gpui::TestAppContext) {
    let (mut server, view, cx) = held_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.flush_pending_input(cx), "the focused pane is recorded");
            view.options.surface_size.cols += 1;
            assert!(!view.input_ready());
            drop_on_second_pane(view, window, cx);
            view.options.surface_size.cols -= 1;
            assert!(view.input_ready());
            view.send(text("sentinel"), cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        server.receive(),
        pane_input("w1:p2", paste("/tmp/held\\ drop.txt"))
    );
    assert_eq!(server.receive(), pane_input("w1:p1", text("sentinel")));
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

/// A snapshot/surface gap clears the frame, so the drop keeps its point and
/// finds its pane in the frame that closes the gap.
#[gpui::test]
fn a_drop_without_a_frame_reaches_the_pane_under_its_point(cx: &mut gpui::TestAppContext) {
    let (mut server, view, cx) = held_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let frame = open_gap(view, cx, "w1:p1");
            assert!(view.live.surface.is_none(), "the gap has no frame");
            drop_on_second_pane(view, window, cx);
            close_gap(view, cx, frame);
            view.send(text("sentinel"), cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        server.receive(),
        pane_input("w1:p2", paste("/tmp/held\\ drop.txt"))
    );
    assert_eq!(server.receive(), pane_input("w1:p1", text("sentinel")));
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui::test]
fn a_drop_whose_point_lands_on_nothing_is_reported(cx: &mut gpui::TestAppContext) {
    let (mut server, view, cx) = held_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut frame = open_gap(view, cx, "w1:p1");
            Arc::make_mut(&mut frame).panes.clear();
            drop_on_second_pane(view, window, cx);
            close_gap(view, cx, frame);
            view.send(text("sentinel"), cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(server.receive(), pane_input("w1:p1", text("sentinel")));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.local_error.as_deref(),
            Some("Files not pasted: nothing under the drop point")
        );
    });
}
