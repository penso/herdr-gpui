use super::*;

#[gpui::test]
fn turning_on_the_qa_menu_closes_the_open_popup(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_application_menu(Some(0), window, cx)
        })
    });
    view.read_with(cx, |view, _| {
        assert!(view.menu.application.is_some());
        assert_eq!(view.menu.page, Some(Page::Application));
    });
    view.update(cx, |view, cx| {
        let mut config = view.config.clone();
        config.features.qa_menu = true;
        let theme = view.theme.clone();
        view.load_gui_config_with(move || Ok((config, theme)), cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.config.features.qa_menu);
        assert!(view.menu.application.is_none());
        assert_eq!(view.menu.page, None);
    });
}

#[gpui::test]
fn a_reload_that_keeps_the_menus_leaves_the_popup_open(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_application_menu(Some(0), window, cx)
        })
    });
    view.update(cx, |view, cx| {
        let config = view.config.clone();
        let theme = view.theme.clone();
        view.load_gui_config_with(move || Ok((config, theme)), cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.menu.application.is_some());
        assert_eq!(view.menu.page, Some(Page::Application));
    });
}
