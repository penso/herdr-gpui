use super::*;

#[gpui::test]
fn ssh_command_dialog_prefills_and_refuses_a_bad_command(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        add_host(&mut view);
        view
    });
    view.update(cx, |view, cx| {
        view.config.devices.insert(
            "0123456789abcdef0123456789abcdef".into(),
            crate::config::DeviceSettings {
                keybindings: Default::default(),
                ssh_command: herdr_client::SshCommand::try_from("tailscale ssh").unwrap(),
            },
        );
        cx.notify();
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update_in(cx, |view, window, cx| {
        view.open_host_menu(HOST, point(px(10.), px(10.)), window, cx)
    });
    if cfg!(windows) {
        assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
        return;
    }
    cx.simulate_keystrokes("down down enter");
    let input = view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::SshCommand));
        host(view).input.clone().unwrap()
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    // The saved command is prefilled and selected, and the field owns typing.
    input.read_with(cx, |input, _| assert_eq!(input.text(), "tailscale ssh"));
    // A refused command keeps the dialog open and says why, without saving.
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_text_selected("-F config", cx)));
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::SshCommand));
        assert!(host(view).error.is_some());
        assert!(!host(view).saving_command);
    });
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
}
