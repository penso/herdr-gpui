//! A move in progress: what ends it while its worker still runs.

use super::start::local_window;
use crate::{
    SheprdWindow,
    chat::{ConvertError, convert::Job, existing::Scope},
    sidebar::layout_tests::fixture_window,
    window::MockPeer,
};
use sheprd_client::protocol::ClientMessage as DaemonMessage;
use std::sync::atomic::Ordering;

/// The scope of the window's current daemon.
fn scope(view: &SheprdWindow) -> Scope {
    let endpoint = &view.endpoints[view.selected_endpoint];
    Scope {
        target: endpoint.connection.target.clone(),
        boot: view.live.snapshot.as_ref().unwrap().boot_id.clone(),
        selection: view.selection_epoch,
        connection: endpoint.generation,
    }
}

#[gpui::test]
fn a_move_keeps_waiting_while_the_window_stays_on_its_host(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&[]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            let (job, _worker, cancelled) = Job::waiting("w1:p1", scope(view));
            view.chats.converting = Some(job);
            view.poll_chat_convert(cx);
            assert!(view.chats.converting.is_some());
            assert!(!cancelled.load(Ordering::Acquire));
        })
    });
}

#[gpui::test]
fn leaving_the_host_stops_the_worker_before_it_answers(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&[]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            let mut left = scope(view);
            left.selection += 1;
            // The worker has answered nothing, and is still pressing keys.
            let (job, _worker, cancelled) = Job::waiting("w1:p1", left);
            view.chats.converting = Some(job);
            view.poll_chat_convert(cx);
            assert!(view.chats.converting.is_none());
            assert!(cancelled.load(Ordering::Acquire));
            assert!(view.flash.is_some());
        })
    });
}

#[gpui::test]
fn a_reconnected_daemon_stops_the_worker_too(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&[]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            let mut restarted = scope(view);
            restarted.boot.push_str("-before");
            let (job, _worker, cancelled) = Job::waiting("w1:p1", restarted);
            view.chats.converting = Some(job);
            view.poll_chat_convert(cx);
            assert!(cancelled.load(Ordering::Acquire));
        })
    });
}

#[gpui::test]
fn turning_chats_off_stops_the_worker(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&[]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            let (job, _worker, cancelled) = Job::waiting("w1:p1", scope(view));
            view.chats.converting = Some(job);
            view.config.features.acp_chats = false;
            view.poll_chat_convert(cx);
            assert!(view.chats.converting.is_none());
            assert!(cancelled.load(Ordering::Acquire));
        })
    });
}

#[gpui::test]
fn a_line_the_worker_finished_is_dropped_after_leaving(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&[]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            let mut left = scope(view);
            left.connection += 1;
            let (job, worker, _cancelled) = Job::waiting("w1:p1", left);
            worker.send(Ok("the line".to_owned())).unwrap();
            view.chats.converting = Some(job);
            view.poll_chat_convert(cx);
            assert!(view.chats.converting.is_none());
            // The fence's flash, not a typed line, is what became of it.
            assert!(view.flash.is_some());
        })
    });
}

#[gpui::test]
fn a_stopped_agents_line_is_typed_into_its_pane(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&[]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            let (job, worker, _cancelled) = Job::waiting("w1:p1", scope(view));
            worker.send(Ok("the line".to_owned())).unwrap();
            view.chats.converting = Some(job);
            view.poll_chat_convert(cx);
            assert!(view.chats.converting.is_none());
            assert!(view.flash.is_none(), "{:?}", view.flash);
        })
    });
    let typed = std::iter::repeat_with(|| peer.receive())
        .take(8)
        .find(|message| matches!(message, DaemonMessage::ClientShellPaneInput { .. }));
    assert!(typed.is_some());
}

#[gpui::test]
fn a_finished_worker_failure_is_reported_once(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&[]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            local_window(view, &peer, cx);
            let (job, worker, _cancelled) = Job::waiting("w1:p1", scope(view));
            worker.send(Err(ConvertError::Moved)).unwrap();
            view.chats.converting = Some(job);
            view.poll_chat_convert(cx);
            assert!(view.chats.converting.is_none());
            assert!(view.flash.is_some());
        })
    });
}
