use super::*;

#[gpui::test]
fn a_window_with_stale_settings_cannot_reset_the_shared_workbench(cx: &mut gpui::TestAppContext) {
    use crate::px0::{Config, Launcher, State};
    let (view, cx) = window(cx);
    let current = WebUrl::try_from("http://localhost:8888/").unwrap();
    cx.update(|_, cx| {
        Launcher::fixture(
            cx,
            Config {
                url: Some(current.clone()),
            },
            State::Ready(current.clone()),
        )
    });
    for old in ["http://localhost:7777/", "http://localhost:9999/"] {
        view.update(cx, |view, _| {
            view.config.px0.url = Some(WebUrl::try_from(old).unwrap())
        });
        cx.update(|window, cx| view.update(cx, |view, cx| view.ensure_px0_page(window, cx)));
        cx.update(|_, cx| {
            assert!(matches!(Launcher::state(cx), State::Ready(url) if url == current))
        });
    }
}

#[gpui::test]
fn px0_opens_in_the_active_group_without_browser_chrome(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| {
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.panes = serde_json::from_value(serde_json::json!([{
            "pane_id": "p0", "workspace_id": "w0", "tab_id": "t0", "label": "shell",
            "cwd": if cfg!(windows) { r"C:\checkout" } else { "/checkout" },
            "foreground_cwd": null, "focused": true, "right_click_passthrough": false
        }]))
        .unwrap();
    });
    let id = cx.update(|window, cx| view.update(cx, |view, cx| view.open_px0(None, window, cx)));
    if !super::super::EMBEDDED {
        assert!(id.is_none());
        return;
    }
    let id = id.unwrap();
    draw(cx);
    assert!(cx.debug_bounds("px0-placeholder").is_some());
    assert!(cx.debug_bounds("browser-reload").is_none());
    let again = cx.update(|window, cx| view.update(cx, |view, cx| view.open_px0(None, window, cx)));
    assert_eq!(again, Some(id));
    assert!(cx.update(|_, cx| view.read(cx).group_shows_page(id, cx)));
    let first = view.read_with(cx, |view, _| view.group_slots()[0].id);
    cx.update(|window, cx| view.update(cx, |view, cx| view.split_group(first, window, cx)));
    let second = view.read_with(cx, |view, _| view.group_slots()[1].id);
    let moved =
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_px0(Some(second), window, cx)));
    assert_eq!(moved, Some(id));
    assert_eq!(
        cx.update(|_, cx| view.read(cx).group_shown(second, cx)),
        crate::browser::Shown::Page(id)
    );
}

#[gpui::test]
fn remote_workspaces_never_open_a_local_px0(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| {
        view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Ssh {
            target: "example.test".into(),
            session: "default".into(),
        };
        assert!(view.px0_root().is_none());
    });
    assert!(
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_px0(None, window, cx)))
            .is_none()
    );
}
