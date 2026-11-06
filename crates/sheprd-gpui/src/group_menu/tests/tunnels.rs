use super::*;
use crate::listening_ports::{Key, Tunnel, parse};
use sheprd_client::ConnectTarget;

/// A remote workspace `w0` listening on loopback alone at 5173 and 5174,
/// so both open through a tunnel, with the menu of its first group open.
fn remote_menu(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> GroupId {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:devbox".into(),
                "Devbox".into(),
                ConnectTarget::Ssh {
                    target: "devbox".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = view.live.snapshot.clone();
            view.endpoints = vec![remote];
            view.selected_endpoint = 0;
            view.listening_ports.seed(
                crate::usage::Host::Ssh("devbox".into()),
                parse("L 1 127.0.0.1:5173 vite\nL 1 127.0.0.1:5174 vite\nE 1 w0\n").unwrap(),
            );
            cx.notify();
        })
    });
    let group = groups(view, cx)[0];
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_group_menu(group, Point::default(), window, cx)
        })
    });
    draw(cx);
    group
}

fn tunnel_key(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    group: GroupId,
    port: u16,
) -> Key {
    actions(view, cx, group)
        .into_iter()
        .find_map(|action| match action {
            Action::Port {
                number,
                link: Link::Tunnel(key),
                ..
            } if number == port => Some(key),
            _ => None,
        })
        .unwrap()
}

#[gpui::test]
fn an_open_tunnel_closes_from_its_menu_row(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let group = remote_menu(&view, cx);
    // No tunnel is open yet, so no row offers to close one.
    assert!(cx.debug_bounds("group-menu-Port5173").is_some());
    assert!(cx.debug_bounds("group-menu-close-Port5173").is_none());
    assert!(cx.debug_bounds("group-menu-close-Port5174").is_none());

    let key = tunnel_key(&view, cx, group, 5173);
    // `sleep` stands in for `ssh`; closing the tunnel kills and reaps it.
    let child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tunnels
                .insert(key.clone(), Tunnel::around(child, 41010));
            cx.notify();
        })
    });
    draw(cx);
    // Only the port whose tunnel is open gets the button.
    let close = cx.debug_bounds("group-menu-close-Port5173").unwrap();
    let row = cx.debug_bounds("group-menu-Port5173").unwrap();
    assert!(close.left() >= row.left() && close.right() <= row.right());
    assert!(cx.debug_bounds("group-menu-close-Port5174").is_none());

    cx.simulate_click(close.center(), Modifiers::none());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(!view.tunnels.is_open(&key));
        // Closing the tunnel neither opens the port nor leaves the menu.
        assert_eq!(view.menu.page, Some(Page::Group));
    });
    assert!(cx.debug_bounds("group-menu-close-Port5173").is_none());
    assert!(cx.debug_bounds("group-menu-Port5173").is_some());
}
