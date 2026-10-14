use super::*;

const ID: &str = "ssh:0123456789abcdef0123456789abcdef";

fn add_ssh(view: &mut HerdrWindow, label: &str, target: &str) {
    view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
    view.endpoints.push(crate::endpoint::Endpoint::new(
        ID.into(),
        label.into(),
        herdr_client::ConnectTarget::Ssh {
            target: target.into(),
            session: "default".into(),
        },
        true,
    ));
}

#[gpui::test]
fn ssh_command_opens_saved_devices_without_enabling_them(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| add_ssh(view, "Devbox", "me@devbox"));
    });
    view.update_in(cx, |view, window, cx| {
        // Missing hosts and non-SSH devices cannot open an executable editor.
        assert_eq!(
            error_code(&ssh_command(view, "ssh:nowhere", window, cx)),
            Some(ErrorCode::NotFound)
        );
        assert_eq!(
            error_code(&ssh_command(view, "local", window, cx)),
            Some(ErrorCode::Unsupported)
        );
        assert_eq!(view.menu.page, None);

        // Editing a disabled profile must not enable or connect it.
        view.endpoints[1].enabled = false;
        for name in [ID, "Devbox"] {
            let opened = ssh_command(view, name, window, cx);
            assert!(matches!(opened, Response::Done { .. }), "{opened:?}");
            assert_eq!(view.menu.page, Some(Page::SshCommand));
            assert!(view.menu.host.is_some());
            assert!(!view.endpoints[1].enabled);
            assert!(view.endpoints[1].connection.handle.is_none());
            view.dismiss_menu(window, cx);
        }
    });
}
