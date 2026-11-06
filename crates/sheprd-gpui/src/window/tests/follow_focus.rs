//! A daemon's snapshot reaching a background connection moves the window
//! through the same tick that keeps it running.
use super::*;

fn on_tab(tab: &str, revision: u64) -> sheprd_client::protocol::ClientShellSnapshot {
    let mut snapshot = snapshot(1);
    snapshot.focused_tab_id = Some(tab.into());
    snapshot.revision = revision;
    snapshot
}

fn deliver(view: &SheprdWindow, snapshot: sheprd_client::protocol::ClientShellSnapshot) {
    view.endpoints[1]
        .connection
        .inbox
        .lock()
        .unwrap()
        .apply(sheprd_client::ClientEvent::Snapshot(Arc::new(snapshot)));
}

#[gpui::test]
fn the_tick_takes_the_window_to_the_host_a_focus_moved(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut homelab = crate::endpoint::Endpoint::new(
            "ssh:homelab".into(),
            "Homelab".into(),
            sheprd_client::ConnectTarget::Ssh {
                target: "me@homelab".into(),
                session: "default".into(),
            },
            true,
        );
        homelab.never_connect();
        view.endpoints.push(homelab);
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            deliver(view, on_tab("t0", 1));
            view.tick(window, cx);
            assert_eq!(view.selected_endpoint, 0, "connecting is not a focus");
            deliver(view, on_tab("t1", 2));
            view.tick(window, cx);
            assert_eq!(view.selected_endpoint, 1);
        });
    });
}

#[gpui::test]
fn composing_counts_as_typing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.last_input.is_none());
            gpui::EntityInputHandler::replace_and_mark_text_in_range(
                view, None, "\u{304b}", None, window, cx,
            );
            assert!(view.last_input.is_some());
        });
    });
}
