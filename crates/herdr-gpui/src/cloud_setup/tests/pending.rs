//! A new device's setup script waiting for a window to show the device, then
//! opening its tab there, and the ways it gives up instead.
#![allow(clippy::unwrap_used)]

use super::{Pending, PendingSetups, Running};
use crate::{
    HerdrWindow, NavigationTarget, cloud::CloudProvider, sidebar::layout_tests::fixture_window,
    window::MockPeer, worktree_scripts::tests::connect,
};
use herdr_client::protocol::{ClientMessage, ClientPaneInputEvent};
use serde_json::json;
use std::time::{Duration, Instant};

fn provider() -> CloudProvider {
    CloudProvider::ALL[0]
}

fn pending(endpoint: &str, now: Instant) -> Pending {
    Pending::new(
        endpoint.into(),
        "Build box".into(),
        provider(),
        "echo hi\n".into(),
        now,
    )
}

/// The local endpoint stands in for the device: the queue matches by ID.
fn local() -> &'static str {
    crate::endpoint::LOCAL
}

#[gpui::test]
fn a_shown_devices_script_opens_its_tab_and_types_the_fixed_line(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    let now = Instant::now();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            connect(view, &peer, cx);
            view.queue_cloud_setup(pending("coder:elsewhere", now), cx);
            view.poll_cloud_setup(now, cx);
            // Another device's script waits for that device.
            assert!(view.cloud_setup.is_none());
            view.queue_cloud_setup(pending(local(), now), cx);
            view.poll_cloud_setup(now, cx);
            assert!(view.cloud_setup.is_some());
            // Taken, so no other window runs it again.
            assert_eq!(PendingSetups::waiting(cx), ["coder:elsewhere"]);
            // One script starts at a time, a worktree's included.
            assert!(view.script_slot_busy());
        })
    });
    let tab = peer.request();
    assert_eq!(tab["method"], "tab.create");
    let params = &tab["params"];
    assert_eq!(params["label"], "setup");
    assert_eq!(params["focus"], true);
    assert!(params.get("cwd").is_none());
    assert_eq!(params["env"]["HERDR_WORKTREE_SCRIPT"], "echo hi\n");
    assert_eq!(params["env"]["HERDR_CLOUD_PROVIDER"], provider().key());
    let created = json!({"result":{"type":"tab_created",
        "tab":{"tab_id":"w1:t9"},"root_pane":{"pane_id":"w1:p9"}}});
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = tab["id"].as_str().unwrap().to_owned();
            view.live.script_response = Some((id, Some(Ok(created))));
            view.poll_cloud_setup_tab(cx);
            assert!(view.cloud_setup.is_none());
            assert!(view.local_error.is_none(), "{:?}", view.local_error);
            assert_eq!(
                view.pending_navigation,
                Some(NavigationTarget::Tab("w1:t9".into()))
            );
        })
    });
    // Other traffic, such as the host theme, may come first.
    let typed = std::iter::repeat_with(|| peer.receive())
        .take(8)
        .find(|message| matches!(message, ClientMessage::ClientShellPaneInput { .. }));
    assert_eq!(
        typed.unwrap(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p9".into(),
            events: vec![
                ClientPaneInputEvent::TextCommit(
                    crate::worktree_scripts::command_line(
                        crate::worktree_scripts::ScriptKind::Setup,
                        false
                    )
                    .into()
                ),
                crate::menu::enter_key(),
            ],
        }
    );
}

#[gpui::test]
fn a_script_waits_while_disconnected_or_another_script_starts(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    let now = Instant::now();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.queue_cloud_setup(pending(local(), now), cx);
            view.poll_cloud_setup(now, cx);
            assert!(view.cloud_setup.is_none());
            connect(view, &peer, cx);
            view.cloud_setup = Some(Running {
                boot: "boot-v1".into(),
                endpoint: (view.selection_epoch, view.endpoints[0].generation),
                endpoint_id: local().into(),
                label: "Other".into(),
                request: "busy".into(),
            });
            view.poll_cloud_setup(now, cx);
            assert_eq!(PendingSetups::waiting(cx), [local()]);
        })
    });
}

#[gpui::test]
fn a_device_queued_again_replaces_its_script_and_old_ones_give_up(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let now = Instant::now();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.queue_cloud_setup(pending("coder:a", now), cx);
            view.queue_cloud_setup(pending("coder:a", now), cx);
            assert_eq!(PendingSetups::waiting(cx), ["coder:a"]);
            assert!(view.flash.is_none());
            // Never shown: given up on, with a word.
            view.poll_cloud_setup(now + Duration::from_secs(31 * 60), cx);
            assert!(PendingSetups::waiting(cx).is_empty());
            let (flash, _) = view.flash.as_ref().unwrap();
            assert!(flash.text.contains("did not run"), "{}", flash.text);
        })
    });
}

#[gpui::test]
fn a_full_queue_drops_its_oldest_script(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let now = Instant::now();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut index = 0;
            while view.flash.is_none() {
                assert!(index < 64, "the queue never filled");
                view.queue_cloud_setup(pending(&format!("coder:{index}"), now), cx);
                index += 1;
            }
            let waiting = PendingSetups::waiting(cx);
            assert_eq!(waiting.len(), index - 1);
            assert!(!waiting.contains(&"coder:0".to_owned()));
        })
    });
}

#[gpui::test]
fn a_replaced_connection_fails_the_script_on_screen(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            connect(view, &peer, cx);
            view.cloud_setup = Some(Running {
                boot: "an-older-boot".into(),
                endpoint: (view.selection_epoch, view.endpoints[0].generation),
                endpoint_id: local().into(),
                label: "Build box".into(),
                request: "r1".into(),
            });
            view.poll_cloud_setup_tab(cx);
            assert!(view.cloud_setup.is_none());
            let error = view
                .local_error
                .clone()
                .unwrap()
                .replace(['\u{2068}', '\u{2069}'], "");
            assert!(error.contains("setup script for Build box"), "{error}");
        })
    });
}

fn add_cloud(view: &mut HerdrWindow) -> String {
    let id = format!("{}:w1", provider().key());
    view.endpoints.push(crate::endpoint::Endpoint::new(
        id.clone(),
        "Build box".into(),
        herdr_client::ConnectTarget::Cloud {
            provider: provider(),
            account: "https://cloud.example.com".into(),
            id: "w1".into(),
            machine: "herdr-tidy-lark-aa".into(),
            session: "default".into(),
        },
        true,
    ));
    id
}

#[gpui::test]
fn running_setup_again_needs_a_cloud_device_and_a_script(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = add_cloud(view);
            assert!(matches!(
                view.rerun_cloud_setup(local(), cx),
                Err(crate::Error::CloudSetup(super::super::Error::NoDevice(_)))
            ));
            assert!(matches!(
                view.rerun_cloud_setup(&id, cx),
                Err(crate::Error::CloudSetup(super::super::Error::NoScript(_)))
            ));
            assert!(PendingSetups::waiting(cx).is_empty());
            view.config.cloud.setup = Some("echo again".into());
            view.rerun_cloud_setup(&id, cx).unwrap();
            assert_eq!(PendingSetups::waiting(cx), [id]);
        })
    });
}

#[gpui::test]
fn a_finished_job_queues_its_script_and_a_failed_one_does_not(cx: &mut gpui::TestAppContext) {
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let (view, cx) = cx.add_window_view(fixture_window);
    let device = |id: &str| crate::cloud::SavedDevice {
        provider: provider(),
        id: id.into(),
        label: format!("Box {id}"),
        account: "https://cloud.example.com".into(),
        machine: id.into(),
        session: "default".into(),
        enabled: true,
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let (made, plain) = (device("made"), device("plain"));
            for (name, saved, script) in [
                ("made", Ok(made), Some("echo made".to_owned())),
                ("plain", Ok(plain), None),
                (
                    "failed",
                    Err(crate::Error::from(crate::cloud::Error::Cancelled)),
                    Some("echo no".to_owned()),
                ),
            ] {
                view.start_cloud_job(provider(), name.into(), script, move |_, _| saved, cx)
                    .unwrap();
            }
        })
    });
    // The jobs' workers finish at once; their results arrive on the drain.
    for _ in 0..200 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        if view.read_with(cx, |view, _| view.cloud_jobs.is_empty()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    view.read_with(cx, |view, _| assert!(view.cloud_jobs.is_empty()));
    let waiting = cx.update(|_, cx| PendingSetups::waiting(cx));
    assert_eq!(waiting, [format!("{}:made", provider().key())]);
}

#[gpui::test]
fn a_script_waits_for_the_devices_first_surface(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    let now = Instant::now();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            connect(view, &peer, cx);
            // Connected with a snapshot, but the daemon has not sent this
            // client a surface yet, so it would refuse `tab.create`.
            let surface = view.live.surface.take();
            view.queue_cloud_setup(pending(local(), now), cx);
            view.poll_cloud_setup(now, cx);
            assert!(view.cloud_setup.is_none());
            assert_eq!(PendingSetups::waiting(cx), [local()]);
            view.live.surface = surface;
            view.poll_cloud_setup(now, cx);
            assert!(view.cloud_setup.is_some());
            assert!(PendingSetups::waiting(cx).is_empty());
        })
    });
}
