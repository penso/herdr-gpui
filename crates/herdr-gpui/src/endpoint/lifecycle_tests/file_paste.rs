//! Cmd-V of files copied in a file manager behaves like dropping them.
use super::*;
use gpui::{ClipboardEntry, ExternalPaths};

/// Finder publishes the copied files and, as text, only their names.
pub(super) fn file_copy(paths: &[PathBuf], name: &str) -> ClipboardItem {
    let mut entries = vec![ClipboardEntry::ExternalPaths(ExternalPaths(
        paths.iter().cloned().collect(),
    ))];
    entries.extend(ClipboardItem::new_string(name.into()).into_entries());
    ClipboardItem { entries }
}

pub(super) fn cmd_v() -> gpui::KeyDownEvent {
    gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("cmd-v").unwrap(),
        is_held: false,
        prefer_character_input: false,
    }
}

#[gpui::test]
fn copied_files_paste_their_escaped_paths_into_a_local_pane(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let directory = tempfile::tempdir().unwrap();
    let image = directory
        .path()
        .join("Screenshot 2026-10-14 at 11.01.09 AM.png");
    let quoted = directory.path().join("it's.txt");
    let mut directory_text = String::new();
    crate::terminal::escape_into(directory.path().to_str().unwrap(), &mut directory_text);
    let separator = std::path::MAIN_SEPARATOR_STR.replace('\\', "\\\\");
    let expected = format!(
        "{directory_text}{separator}Screenshot\\ 2026-10-14\\ at\\ 11.01.09\\ AM.png \
         {directory_text}{separator}it\\'s.txt"
    );
    // Cmd-V reads the clipboard on a background task on macOS and through
    // GPUI's own clipboard elsewhere; both paste the paths, never the name.
    for keyed in [true, false] {
        let (endpoint, mut server) = connected_endpoint("image");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                prepare_mouse(view, endpoint, cx);
                assert!(!view.accepts_remote_images());
                let item = file_copy(&[image.clone(), quoted.clone()], "Screenshot.png");
                if keyed {
                    cx.write_to_clipboard(item);
                    view.key_down(&cmd_v(), window, cx);
                } else {
                    assert!(view.paste_terminal_clipboard(item, false, cx));
                }
                view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
            });
        });
        cx.run_until_parked();
        // A local paste keeps its place ahead of input typed after Cmd-V.
        for event in [
            ClientPaneInputEvent::Paste(expected.clone()),
            ClientPaneInputEvent::TextCommit("sentinel".into()),
        ] {
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellPaneInput {
                    pane_id: "w1:p1".into(),
                    events: vec![event],
                },
                "keyed={keyed}"
            );
        }
        wait_image_finished(&view, cx);
        view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
    }
}

#[gpui::test]
fn one_copied_image_uses_the_image_bridge_on_a_remote_pane(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("shot.PNG");
    std::fs::write(&image, [1, 2, 3]).unwrap();
    for keyed in [true, false] {
        let (endpoint, mut server) = connected_endpoint("ssh:image");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                prepare_remote_image(view, endpoint, cx);
                let item = file_copy(std::slice::from_ref(&image), "shot.PNG");
                if keyed {
                    cx.write_to_clipboard(item);
                    view.key_down(&cmd_v(), window, cx);
                } else {
                    assert!(view.paste_terminal_clipboard(item, false, cx));
                }
            });
        });
        cx.run_until_parked();
        assert_eq!(
            server.receive(),
            ClientMessage::ClipboardImage {
                target: ClientClipboardImageTarget::Pane("w1:p1".into()),
                extension: "png".into(),
                data: vec![1, 2, 3],
            },
            "keyed={keyed}"
        );
        wait_image_finished(&view, cx);
        view.read_with(cx, |view, _| {
            assert!(view.local_error.is_none());
            assert!(view.file_transfer.is_none());
        });
    }
}

#[gpui::test]
fn unpastable_file_paths_report_like_a_drop_and_paste_nothing(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for keyed in [true, false] {
        let (endpoint, mut server) = connected_endpoint("image");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                prepare_mouse(view, endpoint, cx);
                view.local_error = None;
                let item = file_copy(&["/tmp/bad\nname.png".into()], "bad name.png");
                if keyed {
                    cx.write_to_clipboard(item);
                    view.key_down(&cmd_v(), window, cx);
                } else {
                    assert!(view.paste_terminal_clipboard(item, false, cx));
                }
            });
        });
        cx.run_until_parked();
        wait_image_finished(&view, cx);
        view.update(cx, |view, cx| {
            assert_eq!(
                view.local_error.as_deref(),
                Some(
                    format!(
                        "Files not pasted: {}",
                        crate::window::file_drop::Error::Control
                    )
                    .as_str()
                ),
                "keyed={keyed}"
            );
            view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
        });
        // Neither the paths nor the name text was pasted before the sentinel.
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![ClientPaneInputEvent::TextCommit("sentinel".into())],
            },
            "keyed={keyed}"
        );
    }
}

/// Cmd-V reads the clipboard in the background. If the SSH pane stops taking
/// input meanwhile (a resize), its files are refused, never pasted as local
/// paths that name nothing on the remote host.
#[gpui::test]
fn a_remote_pane_that_turns_unready_never_gets_local_paths(cx: &mut gpui::TestAppContext) {
    // Elsewhere GPUI's clipboard is read synchronously, with readiness checked first.
    if cfg!(target_os = "linux") {
        return;
    }
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let directory = tempfile::tempdir().unwrap();
    let files = [
        directory.path().join("a.png"),
        directory.path().join("b.txt"),
    ];
    for path in &files {
        std::fs::write(path, [1, 2, 3]).unwrap();
    }
    // One image would take the bridge; two files would take the SSH copy.
    for copied in [&files[..1], &files[..]] {
        let (endpoint, mut server) = connected_endpoint("ssh:image");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                prepare_remote_image(view, endpoint, cx);
                view.local_error = None;
                cx.write_to_clipboard(file_copy(copied, "a.png"));
                view.key_down(&cmd_v(), window, cx);
                assert_eq!(view.pending_images.len(), 1);
                // A resize is pending: the pane is no longer ready for input.
                view.options.surface_size.cols += 1;
                assert!(!view.input_ready() && view.selected_is_remote());
            });
        });
        cx.run_until_parked();
        wait_image_finished(&view, cx);
        view.update(cx, |view, cx| {
            assert_eq!(
                view.local_error.as_deref(),
                Some(format!("Files not pasted: {}", crate::Error::ConnectionNotReady).as_str()),
                "{} files",
                copied.len()
            );
            assert!(view.file_transfer.is_none());
            view.options.surface_size.cols -= 1;
            assert!(view.input_ready());
            view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
        });
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![ClientPaneInputEvent::TextCommit("sentinel".into())],
            },
            "{} files",
            copied.len()
        );
    }
}
