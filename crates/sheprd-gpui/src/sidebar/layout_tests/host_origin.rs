use super::*;

/// Each host header leads with the mark of where the device runs, between the
/// fold arrow and the name, without pushing the name into the status dot.
#[gpui::test]
fn host_headers_lead_with_the_device_origin_mark(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot(1)));
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:test".into(),
            "Remote".into(),
            ConnectTarget::Ssh {
                target: "me@100.84.59.116".into(),
                session: "default".into(),
            },
            true,
        );
        remote.live.snapshot = view.live.snapshot.clone();
        view.endpoints.push(remote);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = full_draw(window, cx);
    });
    for (id, header, mark, status) in [
        (
            "local",
            "host-local",
            "host-origin-local",
            "host-status-local",
        ),
        (
            "ssh:test",
            "host-ssh:test",
            "host-origin-ssh:test",
            "host-status-ssh:test",
        ),
    ] {
        let header = cx.debug_bounds(header).unwrap();
        let mark = cx
            .debug_bounds(mark)
            .unwrap_or_else(|| panic!("missing mark for {id}"));
        let status = cx.debug_bounds(status).unwrap();
        assert!(header.contains(&mark.center()), "{id}");
        assert_eq!(mark.size.width, mark.size.height, "{id}");
        assert!(mark.size.width > px(0.), "{id}");
        // The fold arrow keeps the header's leading column.
        assert!(
            mark.left() > header.left() + px(super::super::HOST_ARROW_WIDTH),
            "{id}"
        );
        assert!(mark.right() < status.left(), "{id}");
    }
    let mark = cx.debug_bounds("host-origin-ssh:test").unwrap();
    let label = cx.debug_bounds("host-label-ssh:test").unwrap();
    let status = cx.debug_bounds("host-status-ssh:test").unwrap();
    assert!(mark.right() <= label.left());
    assert!(label.right() <= status.left());
}

fn remote_fixture(window: &mut Window, cx: &mut Context<SheprdWindow>) -> SheprdWindow {
    let mut view = fixture_window(window, cx);
    view.live.snapshot = Some(Arc::new(snapshot(1)));
    let mut remote = crate::endpoint::Endpoint::new(
        "ssh:test".into(),
        "Remote".into(),
        ConnectTarget::Ssh {
            target: "me@100.84.59.116".into(),
            session: "default".into(),
        },
        true,
    );
    remote.live.snapshot = view.live.snapshot.clone();
    view.endpoints.push(remote);
    view
}

/// Turned off, the mark goes and the name starts where it was, keeping its
/// right edge.
#[gpui::test]
fn turning_the_device_icons_off_gives_the_name_their_room(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| remote_fixture(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let mark = cx.debug_bounds("host-origin-ssh:test").unwrap();
    let shown = cx.debug_bounds("host-label-ssh:test").unwrap();

    view.update(cx, |view, cx| {
        view.config.show_device_icons = false;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("host-origin-ssh:test").is_none());
    assert!(cx.debug_bounds("host-origin-local").is_none());
    let hidden = cx.debug_bounds("host-label-ssh:test").unwrap();
    assert_eq!(hidden.left(), mark.left());
    assert_eq!(hidden.right(), shown.right());
}

/// Orca and Minimal draw no icons on their rows, so their headers draw no
/// mark even with the setting on.
#[gpui::test]
fn layouts_without_row_icons_draw_no_device_mark(cx: &mut gpui::TestAppContext) {
    for mode in crate::config::LayoutMode::ALL {
        let (_view, cx) = cx.add_window_view(|window, cx| {
            let mut view = remote_fixture(window, cx);
            view.config.layout.mode = mode;
            view
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.run_until_parked();
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        assert!(cx.debug_bounds("host-label-ssh:test").is_some(), "{mode}");
        assert_eq!(
            cx.debug_bounds("host-origin-ssh:test").is_some(),
            mode.device_icons(),
            "{mode}"
        );
    }
}
