use super::{super::Forwarding, tokens};
use crate::{
    SheprdWindow,
    browser::{Location, Store, WebUrl},
    port_forward::State,
    sidebar::layout_tests::{fixture_window, snapshot},
};
use gpui::{Entity, VisualTestContext};
use std::{
    num::NonZeroU16,
    sync::Arc,
    time::{Duration, Instant},
};

/// The fixture window, whose workspace `w1` carries `pairs` as tokens.
fn window<'a>(
    cx: &'a mut gpui::TestAppContext,
    pairs: &[(&str, &str)],
) -> (Entity<SheprdWindow>, &'a mut VisualTestContext) {
    let pairs = tokens(pairs);
    cx.add_window_view(move |window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = snapshot(2);
        shown.focused_workspace_id = Some("w0".into());
        shown.workspaces[1].tokens = pairs;
        view.live.snapshot = Some(Arc::new(shown));
        view
    })
}

fn poll(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.poll_remote_browser(window, cx)));
}

/// Each tab's workspace, opening pane, and address.
fn tabs(cx: &mut VisualTestContext) -> Vec<(String, Option<String>, String)> {
    cx.update(|_, cx| {
        cx.try_global::<Store>()
            .map(|store| {
                store
                    .tabs()
                    .map(|tab| {
                        let address = match &tab.location {
                            Some(Location::Web { url }) => url.as_str().to_owned(),
                            other => format!("{other:?}"),
                        };
                        (tab.workspace_id.clone(), tab.origin.clone(), address)
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

#[gpui::test]
fn a_request_opens_its_page_in_its_workspace_once(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(
        cx,
        &[
            ("herdr-gpui-9-1", "open-background w1:p3"),
            ("herdr-gpui-9-1_0", "localhost:3000/app"),
        ],
    );
    poll(&view, cx);
    // The fixture's daemon is this machine's, so its localhost is ours.
    let opened = vec![(
        "w1".to_owned(),
        Some("w1:p3".to_owned()),
        "http://localhost:3000/app".to_owned(),
    )];
    assert_eq!(tabs(cx), opened);
    // The tokens stay until they expire; the request is not served again,
    // even once the user closed its tab.
    cx.update(|_, cx| {
        let id = cx.global::<Store>().tabs().next().unwrap().id;
        Store::update(cx, |store| store.close(id));
    });
    poll(&view, cx);
    assert!(tabs(cx).is_empty());
}

#[gpui::test]
fn an_address_that_cannot_open_is_flashed(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(
        cx,
        &[
            ("herdr-gpui-9-2", "open"),
            ("herdr-gpui-9-2_0", "javascript:alert(1)"),
        ],
    );
    poll(&view, cx);
    assert!(tabs(cx).is_empty());
    view.read_with(cx, |view, _| assert!(view.flash.is_some()));
}

fn forwarding(endpoint: String, until: Instant) -> Forwarding {
    Forwarding {
        endpoint,
        workspace: "w1".into(),
        pane: Some("w1:p3".into()),
        url: WebUrl::from_typed("localhost:3000/app?x=1").unwrap(),
        target: "dev@build".into(),
        port: NonZeroU16::new(3000).unwrap(),
        focus: false,
        until,
    }
}

#[gpui::test]
fn a_loopback_page_opens_once_its_forward_listens(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, &[]);
    view.update(cx, |view, _| {
        let endpoint = view.endpoints[0].id.clone();
        let later = Instant::now() + Duration::from_secs(60);
        view.remote_browser.push(forwarding(endpoint, later));
        view.port_forwards
            .fixture("dev@build", 3000, State::Starting);
    });
    poll(&view, cx);
    assert!(tabs(cx).is_empty());
    view.update(cx, |view, _| {
        assert_eq!(view.remote_browser.len(), 1);
        view.port_forwards.stop_all();
        view.port_forwards
            .fixture("dev@build", 3000, State::Listening { local_port: 13000 });
    });
    poll(&view, cx);
    assert_eq!(
        tabs(cx),
        vec![(
            "w1".to_owned(),
            Some("w1:p3".to_owned()),
            "http://127.0.0.1:13000/app?x=1".to_owned(),
        )]
    );
    view.read_with(cx, |view, _| assert!(view.remote_browser.is_empty()));
}

#[gpui::test]
fn a_forward_that_ends_or_stalls_drops_its_page(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, &[]);
    view.update(cx, |view, _| {
        let endpoint = view.endpoints[0].id.clone();
        view.remote_browser
            .push(forwarding(endpoint.clone(), Instant::now()));
        view.port_forwards
            .fixture("dev@build", 3000, State::Starting);
    });
    poll(&view, cx);
    view.read_with(cx, |view, _| {
        assert!(view.remote_browser.is_empty());
        assert!(view.flash.is_some());
    });
    view.update(cx, |view, _| {
        let endpoint = view.endpoints[0].id.clone();
        let later = Instant::now() + Duration::from_secs(60);
        view.remote_browser.push(forwarding(endpoint, later));
        view.port_forwards.stop_all();
        view.port_forwards
            .fixture("dev@build", 3000, State::Ended("gone".into()));
    });
    poll(&view, cx);
    view.read_with(cx, |view, _| assert!(view.remote_browser.is_empty()));
    assert!(tabs(cx).is_empty());
}
