use super::*;
use crate::menu::DeviceScope;

/// Rows in the picker: All Devices, then each endpoint in catalog order.
const LOCAL_ROW: &str = "device-row-1";
const A_ROW: &str = "device-row-2";
const B_ROW: &str = "device-row-3";

fn remote(id: &str, view: &SheprdWindow) -> crate::endpoint::Endpoint {
    let mut endpoint = crate::endpoint::Endpoint::new(
        id.into(),
        id.into(),
        ConnectTarget::Ssh {
            target: "example.invalid".into(),
            session: "default".into(),
        },
        true,
    );
    endpoint.live.snapshot = view.live.snapshot.clone();
    endpoint
}

fn scope<const N: usize>(ids: [&str; N]) -> DeviceScope {
    DeviceScope::Only(ids.into_iter().map(str::to_owned).collect())
}

#[gpui::test]
fn the_device_picker_lists_several_devices_at_once(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            for id in ["ssh:a", "ssh:b"] {
                let endpoint = remote(id, view);
                view.endpoints.push(endpoint);
            }
        })
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    };
    draw(cx);
    let picker = cx.debug_bounds("device-picker").unwrap().center();
    let row =
        |cx: &mut gpui::VisualTestContext, selector| cx.debug_bounds(selector).unwrap().center();
    let filter =
        |cx: &mut gpui::VisualTestContext| view.read_with(cx, |view, _| view.device_filter.clone());

    // From All Devices, a secondary click lists the picked device beside the
    // one on screen, and the picker stays open for more.
    cx.simulate_click(picker, Modifiers::default());
    draw(cx);
    let a = row(cx, A_ROW);
    cx.simulate_click(a, Modifiers::secondary_key());
    draw(cx);
    assert_eq!(filter(cx), scope(["local", "ssh:a"]));
    assert!(cx.debug_bounds("menu-panel").is_some());
    assert!(cx.debug_bounds("device-current-0").is_none());
    assert!(cx.debug_bounds("device-current-1").is_some());
    assert!(cx.debug_bounds("device-current-2").is_some());
    assert!(cx.debug_bounds("device-current-3").is_none());
    assert!(cx.debug_bounds("host-ssh:a").is_some());
    assert!(cx.debug_bounds("host-ssh:b").is_none());

    // Space toggles the highlighted row; naming every device is All Devices.
    // Hovering a row highlights it, so the keys start from the one under it.
    cx.simulate_mouse_move(a, None, Modifiers::default());
    cx.simulate_keystrokes("down space");
    draw(cx);
    assert_eq!(filter(cx), DeviceScope::All);
    assert!(cx.debug_bounds("device-current-0").is_some());

    // Taking away the device on screen selects the first one still listed.
    let local = row(cx, LOCAL_ROW);
    cx.simulate_click(local, Modifiers::secondary_key());
    draw(cx);
    assert_eq!(filter(cx), DeviceScope::only("local"));
    let b = row(cx, B_ROW);
    cx.simulate_click(b, Modifiers::secondary_key());
    draw(cx);
    assert_eq!(filter(cx), scope(["local", "ssh:b"]));
    let local = row(cx, LOCAL_ROW);
    cx.simulate_click(local, Modifiers::secondary_key());
    draw(cx);
    assert_eq!(filter(cx), DeviceScope::only("ssh:b"));
    view.read_with(cx, |view, _| {
        assert_eq!(view.endpoints[view.selected_endpoint].id, "ssh:b");
    });
    // Switching device closes the picker, as any switch does.
    assert!(cx.debug_bounds("menu-panel").is_none());
    cx.simulate_click(picker, Modifiers::default());
    draw(cx);
    // The last listed device cannot be taken away.
    let b = row(cx, B_ROW);
    cx.simulate_click(b, Modifiers::secondary_key());
    draw(cx);
    assert_eq!(filter(cx), DeviceScope::only("ssh:b"));

    // A plain click still lists one device and closes the picker.
    let a = row(cx, A_ROW);
    cx.simulate_click(a, Modifiers::default());
    draw(cx);
    assert_eq!(filter(cx), DeviceScope::only("ssh:a"));
    assert!(cx.debug_bounds("menu-panel").is_none());
}
