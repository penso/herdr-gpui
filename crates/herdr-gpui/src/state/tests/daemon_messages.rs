use super::*;

#[test]
fn window_titles_are_sanitized_capped_and_cleared() {
    let mut state = LiveState {
        status: ConnectionStatus::Connected,
        ..LiveState::default()
    };
    let title = |title: &str| {
        ClientEvent::Message(ServerMessage::WindowTitle {
            title: Some(title.into()),
        })
    };
    state.apply(title("  agent\u{1b}]2;evil\u{7}\u{9c}\r\n done  "));
    assert_eq!(state.window_title.as_deref(), Some("agent]2;evil done"));

    state.apply(title(&"é".repeat(MAX_WINDOW_TITLE_CHARS + 50)));
    assert_eq!(
        state.window_title.as_ref().map(|t| t.chars().count()),
        Some(MAX_WINDOW_TITLE_CHARS)
    );

    // Nothing printable left is no title, as is an explicit clear.
    state.apply(title("\u{1b}\u{7} \t"));
    assert_eq!(state.window_title, None);
    state.apply(title("x"));
    state.apply(ClientEvent::Message(ServerMessage::WindowTitle {
        title: None,
    }));
    assert_eq!(state.window_title, None);

    // An unchanged title does not wake the window.
    state.apply(title("same"));
    state.dirty = false;
    state.apply(title("same"));
    assert!(!state.dirty);
}

#[test]
fn keyboard_report_all_follows_the_daemon_and_ends_with_the_connection() {
    let report =
        |enabled| ClientEvent::Message(ServerMessage::ClientShellKeyboardReportAll { enabled });
    let mut state = LiveState::default();
    state.apply(ClientEvent::Snapshot(snapshot()));
    state.dirty = false;
    state.apply(report(true));
    assert!(state.keyboard_report_all && state.dirty);
    // A repeated report changes nothing to draw.
    state.dirty = false;
    state.apply(report(true));
    assert!(!state.dirty);
    // It only shapes later key events, so the chrome is spared.
    let mut next = state.clone();
    next.apply(report(false));
    assert!(!next.keyboard_report_all);
    assert!(state.only_surface_changed(&next));
    state.apply(ClientEvent::Disconnected {
        reason: "gone".into(),
    });
    assert!(!state.keyboard_report_all);
}

#[test]
fn messages_meant_for_the_tui_change_nothing() {
    let mut state = LiveState::default();
    state.apply(ClientEvent::Snapshot(snapshot()));
    let before = state.clone();
    state.dirty = false;
    for message in [
        // Shell clients are notified through SemanticNotification.
        ServerMessage::Notify {
            kind: herdr_client::protocol::NotifyKind::Toast,
            message: "Agent finished".into(),
            body: None,
        },
        // Each surface pane carries its own mouse_reporting.
        ServerMessage::MouseCapture {
            enabled: true,
            sgr_pixels: true,
        },
        ServerMessage::DirectTerminalKeyboardProtocol {
            flags: 31,
            modify_other_keys_level: 2,
        },
    ] {
        state.apply(ClientEvent::Message(message));
    }
    assert!(!state.dirty);
    assert!(before.only_surface_changed(&state));
    assert!(state.notifications.is_empty() && state.sound_events.is_empty());
    assert!(!state.keyboard_report_all);
}

#[test]
fn daemon_clipboard_payloads_are_decoded_bounded_and_dropped_when_invalid() {
    let mut state = LiveState::default();
    state.apply(ClientEvent::Message(ServerMessage::Clipboard {
        data: "aGVsbG8=".into(),
    }));
    state.apply(ClientEvent::Message(ServerMessage::Clipboard {
        data: "not base64!".into(),
    }));
    assert_eq!(
        state
            .clipboard_writes
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["hello"]
    );
    // A pane that spams OSC 52 cannot grow the mailbox without bound, and
    // only the newest writes survive: the seeded "hello" is evicted first.
    for index in 0..crate::osc52::MAX_PENDING * 2 {
        state.apply(ClientEvent::Message(ServerMessage::Clipboard {
            data: "eA==".into(),
        }));
        assert_eq!(
            state.clipboard_writes.len(),
            (index + 2).min(crate::osc52::MAX_PENDING)
        );
    }
    assert!(state.clipboard_writes.iter().all(|text| text == "x"));
}
