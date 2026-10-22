use super::*;
use herdr_client::ConnectTarget;

#[gpui::test]
fn a_wsl_pane_is_given_mapped_paths_and_never_a_file_copy(cx: &mut gpui::TestAppContext) {
    // The fixture never renders the window, so no resize replaces the error.
    let (fixture, cx) = cx.add_window_view(|window, cx| DropFixture {
        view: cx.new(|cx| fixture_window(window, cx)),
        submitted_at: None,
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.view.clone());
    view.update(cx, |view, cx| {
        prepare(view, cx);
        // Refused before `wsl.exe` would start, so the route is visible
        // without a distribution.
        view.endpoints[0].connection.target = ConnectTarget::Wsl {
            distro: "a;b".into(),
            session: "default".into(),
        };
        let dropped = [PathBuf::from("C:/Users/me/report.pdf")];
        assert_ne!(
            view.paste_files(InputTarget::Pane("pane-0".into()), &dropped, cx),
            Delivery::Refused
        );
        assert!(view.file_transfer.is_none());
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.local_error.as_deref(),
            Some("Files not pasted: invalid WSL distribution name")
        );
        assert!(view.file_transfer.is_none());
    });
}

#[gpui::test]
fn a_wsl_pane_that_is_not_ready_refuses_the_files(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        prepare(view, cx);
        view.endpoints[0].connection.target = ConnectTarget::Wsl {
            distro: "Ubuntu".into(),
            session: "default".into(),
        };
        view.live.status = ConnectionStatus::Disconnected;
        let dropped = [PathBuf::from("C:/Users/me/report.pdf")];
        assert_eq!(
            view.paste_files(InputTarget::Pane("pane-0".into()), &dropped, cx),
            Delivery::Refused
        );
        assert_eq!(
            view.local_error.as_deref(),
            Some("Files not pasted: The connection is not ready. Try again.")
        );
    });
}
