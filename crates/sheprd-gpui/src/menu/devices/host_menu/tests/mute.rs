use super::*;
use crate::notification_mutes::Mutes;

fn muted(cx: &gpui::App) -> bool {
    Mutes::of(cx).is_some_and(|mutes| mutes.host(HOST))
}

fn window(cx: &mut TestAppContext) -> (gpui::Entity<crate::SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        add_host(&mut view);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    (view, cx)
}

#[gpui::test]
fn the_host_menu_mutes_an_offline_device_and_marks_its_header(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let mark = "host-muted-ssh:0123456789abcdef0123456789abcdef";
    assert!(cx.debug_bounds(mark).is_none());
    for after in [true, false] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_host_menu(HOST, point(px(10.), px(10.)), window, cx);
                view.activate_host_menu(Action::Mute, window, cx);
                assert_eq!(view.menu.page, Some(Page::Host), "the row is a checkbox");
                view.dismiss_menu(window, cx);
            });
            assert_eq!(muted(cx), after);
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let header = cx.debug_bounds(HOST_HEADER).unwrap();
        let shown = cx.debug_bounds(mark);
        assert_eq!(shown.is_some(), after);
        if let Some(shown) = shown {
            assert!(shown.left() >= header.left() && shown.right() <= header.right());
        }
    }
}

#[gpui::test]
fn a_finished_removal_forgets_the_mute_after_its_window_closed(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let this = view.downgrade();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.set_host_muted(HOST, true, cx));
        window.remove_window();
    });
    drop(view);
    // The window's own context is gone with it.
    let app: &mut TestAppContext = cx;
    app.run_until_parked();
    let finish = |result: crate::Result<crate::Result<()>>, app: &mut TestAppContext| {
        app.update(|cx| {
            crate::SheprdWindow::follow_removal(
                this.clone(),
                gpui::Task::ready(result),
                HOST.into(),
                "m5max-ms".into(),
                false,
                cx,
            );
        });
        app.run_until_parked();
        app.update(|cx| muted(cx))
    };
    let failed = Err(crate::menu::devices::Error::Adding.into());
    assert!(finish(failed, app), "a failed removal keeps it");
    assert!(!finish(Ok(Ok(())), app));
}

#[gpui::test]
fn local_mutes_from_its_menu_and_names_nothing_under_its_label(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_host_menu(crate::endpoint::LOCAL, point(px(10.), px(10.)), window, cx);
            view.activate_host_menu(Action::Mute, window, cx);
            assert_eq!(view.menu.page, Some(Page::Host));
        });
        assert!(Mutes::of(cx).is_some_and(|mutes| mutes.host(crate::endpoint::LOCAL)));
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("host-muted-local").is_some());
    assert!(
        cx.debug_bounds(menu_row(super::super::LOCAL_ACTIONS.len() - 1))
            .is_some()
    );
    assert!(
        cx.debug_bounds(menu_row(super::super::LOCAL_ACTIONS.len()))
            .is_none()
    );
    view.read_with(cx, |view, _| assert!(host(view).target.is_empty()));
}
