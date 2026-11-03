//! A live macOS screenshot thumbnail dropped or pasted on a local pane: only
//! this app may read its temporary file, so its bytes take the image bridge.
use super::file_paste::{cmd_v, file_copy};
use super::*;

/// The file a screenshot thumbnail drag names, under a private directory.
fn thumbnail(root: &std::path::Path) -> PathBuf {
    let directory = root.join("TemporaryItems/NSIRD_screencaptureui_vKTdfp");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join("Screenshot 2026-10-16 at 2.09.54\u{202f}PM.png")
}

#[gpui::test]
fn a_screenshot_thumbnail_takes_the_image_bridge_on_a_local_pane(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let root = tempfile::tempdir().unwrap();
    let image = thumbnail(root.path());
    std::fs::write(&image, [1, 2, 3]).unwrap();
    for keyed in [true, false] {
        let (endpoint, mut server) = connected_endpoint("image");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                prepare_mouse(view, endpoint, cx);
                assert!(!view.selected_is_remote());
                let item = file_copy(std::slice::from_ref(&image), "Screenshot.png");
                if keyed {
                    cx.write_to_clipboard(item);
                    view.key_down(&cmd_v(), window, cx);
                } else {
                    // The drop's own path: a drop calls paste_files directly.
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
        view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
    }
}

#[gpui::test]
fn a_thumbnail_that_cannot_be_read_still_pastes_its_path(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let root = tempfile::tempdir().unwrap();
    // The thumbnail saved, so macOS already removed its temporary file.
    let image = thumbnail(root.path());
    let mut expected = String::new();
    crate::terminal::escape_into(image.to_str().unwrap(), &mut expected);
    let (endpoint, mut server) = connected_endpoint("image");
    view.update(cx, |view, cx| {
        prepare_mouse(view, endpoint, cx);
        let item = file_copy(std::slice::from_ref(&image), "Screenshot.png");
        assert!(view.paste_terminal_clipboard(item, false, cx));
    });
    cx.run_until_parked();
    wait_image_finished(&view, cx);
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::Paste(expected)],
        }
    );
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui::test]
fn a_pasted_thumbnail_reaches_the_pane_before_enter_typed_after_it(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let root = tempfile::tempdir().unwrap();
    let image = thumbnail(root.path());
    std::fs::write(&image, [4, 5, 6]).unwrap();
    let enter = gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
        is_held: false,
        prefer_character_input: false,
    };
    let (endpoint, mut server) = connected_endpoint("image");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            cx.write_to_clipboard(file_copy(std::slice::from_ref(&image), "Screenshot.png"));
            view.key_down(&cmd_v(), window, cx);
            // Enter while the clipboard is still being read queues behind it.
            view.key_down(&enter, window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        server.receive(),
        ClientMessage::ClipboardImage {
            target: ClientClipboardImageTarget::Pane("w1:p1".into()),
            extension: "png".into(),
            data: vec![4, 5, 6],
        }
    );
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![crate::terminal::key_input(&enter, true).unwrap()],
        }
    );
    wait_image_finished(&view, cx);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}
