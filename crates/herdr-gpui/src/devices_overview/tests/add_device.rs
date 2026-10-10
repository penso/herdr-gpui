use super::*;
use crate::menu::Page;
use herdr_client::ConnectTarget;

#[gpui::test]
fn isolated_windows_cannot_open_device_setup(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    for target in [
        ConnectTarget::Socket("/unused-layout-test.sock".into()),
        ConnectTarget::Session {
            name: "default".into(),
            development: true,
        },
    ] {
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.endpoints[0].connection.target = target;
            })
        });
        open(&view, cx);
        let add = cx.debug_bounds("devices-add").unwrap();
        cx.simulate_click(add.center(), gpui::Modifiers::none());
        draw(cx);
        view.read_with(cx, |view, _| {
            assert!(view.menu.page.is_none());
        });
        // The dispatch itself also guards keyboard and stale-frame callers.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_device_setup(window, cx);
                assert!(view.menu.page.is_none());
            })
        });
    }
}

#[gpui::test]
fn add_device_opens_the_platforms_setup_form(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.endpoints[0].connection.target = ConnectTarget::Session {
                name: "default".into(),
                development: false,
            };
        })
    });
    open(&view, cx);
    let add = cx.debug_bounds("devices-add").unwrap();
    cx.simulate_click(add.center(), gpui::Modifiers::none());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.menu.page,
            Some(if cfg!(windows) {
                Page::AddWsl
            } else {
                Page::AddDevice
            })
        );
    });
}
