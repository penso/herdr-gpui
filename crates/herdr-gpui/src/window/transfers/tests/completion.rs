use super::*;

#[gpui::test]
fn success_pastes_quoted_paths_to_captured_pane_or_popup(cx: &mut TestAppContext) {
    for is_popup in [false, true] {
        let (fixture, cx) = cx.add_window_view(fixture);
        let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
        let mut peer = Peer::new();
        view.update(cx, |view, cx| {
            peer.prepare(view);
            let target = if is_popup {
                popup(view, "popup");
                InputTarget::Popup("popup".into())
            } else {
                InputTarget::Pane("w1:p1".into())
            };
            view.start_file_transfer_with(
                target,
                vec!["source".into()],
                |_, _, _, progress| {
                    progress(9, 9);
                    Ok(vec![REMOTE.into()])
                },
                |_, _| panic!("accepted upload was removed"),
                cx,
            );
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_pane_id =
                Some("different-pane".into());
        });
        cx.run_until_parked();
        let events = vec![ClientPaneInputEvent::Paste(
            "'/tmp/herdr-upload.ABCDEF123456/it'\\''s a file'".into(),
        )];
        assert_eq!(
            peer.receive(),
            if is_popup {
                ClientMessage::ClientShellPopupInput {
                    terminal_id: "popup".into(),
                    events,
                }
            } else {
                ClientMessage::ClientShellPaneInput {
                    pane_id: "w1:p1".into(),
                    events,
                }
            }
        );
        view.read_with(cx, |view, _| {
            assert!(view.file_transfer.is_none());
            assert!(view.local_error.is_none());
            assert_eq!(
                view.endpoints[0].toasts.entries.back().unwrap().1.title,
                "Copy complete"
            );
        });
        peer.sentinel(&view, cx);
    }
}

#[gpui::test]
fn stale_or_cancelled_success_cleans_original_host_without_pasting(cx: &mut TestAppContext) {
    for case in 0..4 {
        let (fixture, cx) = cx.add_window_view(fixture);
        let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
        let mut peer = Peer::new();
        let (tx, rx) = mpsc::channel();
        view.update(cx, |view, cx| {
            peer.prepare(view);
            view.start_file_transfer_with(
                InputTarget::Pane("w1:p1".into()),
                vec!["source".into()],
                |_, _, _, _| Ok(vec![REMOTE.into()]),
                move |host, paths| {
                    tx.send((host.to_owned(), paths.to_vec())).unwrap();
                    Ok(())
                },
                cx,
            );
            match case {
                0 => {
                    view.selection_epoch += 1;
                    view.endpoints[0].connection.target = ConnectTarget::Ssh {
                        target: "other.invalid".into(),
                        session: "default".into(),
                    };
                }
                1 => view.file_transfer.as_ref().unwrap().cancel(),
                2 => view.menu.page = Some(crate::menu::Page::Palette),
                3 => view.pending_toast = Some(1),
                _ => unreachable!(),
            }
        });
        cx.run_until_parked();
        assert_eq!(rx.try_recv().unwrap(), (HOST.into(), vec![REMOTE.into()]));
        view.read_with(cx, |view, _| assert!(view.file_transfer.is_none()));
        peer.sentinel(&view, cx);
    }
}

#[gpui::test]
fn old_completion_preserves_replacement_and_reports_cleanup_failure(cx: &mut TestAppContext) {
    let (fixture, cx) = cx.add_window_view(fixture);
    let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
    let mut peer = Peer::new();
    let (tx, rx) = mpsc::channel();
    let replacement = view.update(cx, |view, cx| {
        peer.prepare(view);
        view.endpoints[0].label = "Original\n host".into();
        view.start_file_transfer_with(
            InputTarget::Pane("w1:p1".into()),
            vec!["source".into()],
            |_, _, _, _| Ok(vec![REMOTE.into()]),
            move |host, paths| {
                tx.send((host.to_owned(), paths.to_vec())).unwrap();
                Err(herdr_client::Error::UploadCleanupPath)
            },
            cx,
        );
        let original = view.file_transfer.as_ref().unwrap().cancelled.clone();
        let replacement = pending(view, InputTarget::Pane("w1:p1".into()));
        let token = replacement.cancelled.clone();
        view.file_transfer = Some(replacement);
        view.endpoints[0].label = "Replacement host".into();
        assert!(original.load(Ordering::Acquire));
        token
    });
    cx.run_until_parked();
    assert_eq!(rx.try_recv().unwrap(), (HOST.into(), vec![REMOTE.into()]));
    view.read_with(cx, |view, _| {
        assert!(Arc::ptr_eq(
            &replacement,
            &view.file_transfer.as_ref().unwrap().cancelled
        ));
        assert!(!replacement.load(Ordering::Acquire));
        let notice = &view.endpoints[0].toasts.entries.back().unwrap().1;
        assert_eq!(notice.title, "Remote cleanup failed");
        assert_eq!(notice.body.as_deref(), Some("No path was pasted. Temporary files may remain on the original host (Original host)."));
    });
    peer.sentinel(&view, cx);
}

#[gpui::test]
fn backend_cleanup_failure_survives_host_switch_without_disclosing_diagnostics(
    cx: &mut TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(fixture);
    let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
    let mut peer = Peer::new();
    view.update(cx, |view, cx| {
        peer.prepare(view);
        view.endpoints[0].label = "Original\n host".into();
        view.start_file_transfer_with(
            InputTarget::Pane("w1:p1".into()),
            vec!["/private/source".into()],
            |_, _, _, _| {
                Err(herdr_client::Error::UploadCleanup {
                    source: Box::new(herdr_client::Error::UploadIo(std::io::Error::other(
                        "/private/source secret",
                    ))),
                    cleanup: Box::new(herdr_client::Error::UploadIo(std::io::Error::other(
                        "/private/remote secret",
                    ))),
                })
            },
            |_, _| panic!("backend already attempted cleanup; no successful paths exist"),
            cx,
        );
        view.endpoints.push(crate::endpoint::Endpoint::new(
            "other".into(),
            "Other host".into(),
            ConnectTarget::Ssh {
                target: "other.invalid".into(),
                session: "default".into(),
            },
            true,
        ));
        view.selected_endpoint = 1;
        view.selection_epoch += 1;
        view.poll_file_transfer(cx);
        assert!(view.file_transfer.as_ref().unwrap().shown.2);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.file_transfer.is_none());
        assert!(view.local_error.is_none());
        assert!(view.endpoints[0].toasts.entries.is_empty());
        let notices = &view.endpoints[1].toasts.entries;
        assert_eq!(notices.len(), 1);
        let notice = &notices.back().unwrap().1;
        assert_eq!(notice.title, "Remote cleanup failed");
        assert!(notice.visible);
        assert_eq!(notice.body.as_deref(), Some("No path was pasted. Temporary files may remain on the original host (Original host)."));
        for forbidden in ["/private", "secret", HOST, "other.invalid", "Other host"] {
            assert!(!notice.body.as_ref().unwrap().contains(forbidden));
        }
    });
    peer.sentinel(&view, cx);
}
