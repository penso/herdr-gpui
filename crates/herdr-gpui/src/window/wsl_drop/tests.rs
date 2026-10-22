#![allow(clippy::unwrap_used)]

mod held;

use super::*;
use crate::{sidebar::layout_tests::fixture_window, window::MockPeer};
use gpui::{
    AppContext, Entity, IntoElement, Render, TestAppContext, VisualTestContext, Window, div,
};
use herdr_client::{
    ConnectTarget,
    protocol::{ClientMessage, ClientShellPopupSurface},
};

/// Holds the window without drawing it, so no resize traffic reaches the peer.
struct Fixture {
    view: Entity<HerdrWindow>,
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

const PANE: &str = "w1:p1";

/// A connected WSL window with a drop onto its focused pane being mapped.
fn mapping(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, MockPeer, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| Fixture {
        view: cx.new(|cx| fixture_window(window, cx)),
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.view.clone());
    let peer = MockPeer::new();
    view.update(cx, |view, cx| {
        peer.prepare(view, cx);
        view.endpoints[0].connection.target = ConnectTarget::Wsl {
            distro: "Ubuntu".into(),
            session: "default".into(),
        };
        view.flush_pending_input(cx);
        view.wsl_paste = Some(WslPaste {
            token: Arc::new(()),
            endpoint: view.endpoints[0].id.clone(),
            generation: view.endpoints[0].generation,
            epoch: view.selection_epoch,
            boot: view.live.snapshot.as_ref().unwrap().boot_id.clone(),
            target: InputTarget::Pane(PANE.into()),
            mapped: None,
        });
    });
    (view, peer, cx)
}

fn finish(view: &mut HerdrWindow, mapped: &[&str], cx: &mut Context<HerdrWindow>) {
    let token = view.wsl_paste.as_ref().unwrap().token.clone();
    let mapped = mapped.iter().map(|path| (*path).to_owned()).collect();
    view.finish_wsl_paste(&token, Ok(mapped), cx);
}

fn pane_input(events: Vec<ClientPaneInputEvent>) -> ClientMessage {
    ClientMessage::ClientShellPaneInput {
        pane_id: PANE.into(),
        events,
    }
}

/// Nothing else was sent before this marker.
fn sentinel(view: &HerdrWindow, peer: &mut MockPeer) {
    let marker = ClientPaneInputEvent::TextCommit("sentinel".into());
    ConnectionBridge::send_input(
        &peer.client.handle,
        &view.live.snapshot.as_ref().unwrap().boot_id,
        &InputTarget::Pane(PANE.into()),
        marker.clone(),
    )
    .unwrap();
    assert_eq!(peer.receive(), pane_input(vec![marker]));
}

#[gpui::test]
fn keys_typed_while_paths_are_mapped_follow_them(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = mapping(cx);
    view.update(cx, |view, cx| {
        view.send(ClientPaneInputEvent::TextCommit("x".into()), cx);
        // Polling cannot release held keys ahead of the paths.
        assert!(!view.flush_pending_input(cx));
        finish(view, &["/mnt/c/it's here.txt"], cx);
        assert!(view.wsl_paste.is_none());
        assert!(view.local_error.is_none(), "{:?}", view.local_error);
    });
    assert!(matches!(
        peer.receive(),
        ClientMessage::ClientShellPaneInput { pane_id, events }
            if pane_id == PANE && matches!(&events[..], [ClientPaneInputEvent::Paste(text)] if text.contains("/mnt/c/it"))
    ));
    assert_eq!(
        peer.receive(),
        pane_input(vec![ClientPaneInputEvent::TextCommit("x".into())])
    );
}

#[gpui::test]
fn paths_for_a_covered_pane_are_not_pasted(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = mapping(cx);
    view.update(cx, |view, cx| {
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        surface.popup = Some(Box::new(ClientShellPopupSurface {
            terminal_id: "popup".into(),
            title: String::new(),
            width: None,
            height: None,
            frame: surface.frame.clone(),
            mouse_reporting: true,
            sgr_pixel_mouse: false,
            pixel_width: 800,
            pixel_height: 480,
        }));
        finish(view, &["/mnt/c/a"], cx);
        assert_eq!(
            view.local_error.as_deref(),
            Some(
                "Files not pasted: The pane the files were dropped on changed before their paths were ready."
            )
        );
        sentinel(view, &mut peer);
    });
}

#[gpui::test]
fn a_reconnect_forgets_the_drop_and_its_late_answer(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = mapping(cx);
    view.update(cx, |view, cx| {
        let token = view.wsl_paste.as_ref().unwrap().token.clone();
        view.clear_pending_input();
        assert!(view.wsl_paste.is_none());
        view.finish_wsl_paste(&token, Ok(vec!["/mnt/c/a".into()]), cx);
        assert!(view.local_error.is_none());
        sentinel(view, &mut peer);
    });
}

#[gpui::test]
fn a_second_drop_waits_for_the_first(cx: &mut TestAppContext) {
    let (view, _peer, cx) = mapping(cx);
    view.update(cx, |view, cx| {
        let error = view
            .paste_wsl_files(
                InputTarget::Pane(PANE.into()),
                "Ubuntu".into(),
                vec![PathBuf::from("C:/b")],
                cx,
            )
            .unwrap_err();
        assert!(matches!(error, Error::WslPasteBusy));
    });
}

/// The daemon published a snapshot whose surface has not arrived yet.
fn open_gap(view: &mut HerdrWindow) {
    Arc::make_mut(view.live.surface.as_mut().unwrap()).projection_revision += 1;
    assert!(!view.input_ready());
}

fn close_gap(view: &mut HerdrWindow) {
    Arc::make_mut(view.live.surface.as_mut().unwrap()).projection_revision -= 1;
    assert!(view.input_ready());
}

#[gpui::test]
fn paths_mapped_during_a_surface_gap_wait_and_then_lead_the_held_keys(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = mapping(cx);
    view.update(cx, |view, cx| {
        view.send(ClientPaneInputEvent::TextCommit("x".into()), cx);
        open_gap(view);
        finish(view, &["/mnt/c/a"], cx);
        // Nothing is lost or sent while the surface is missing.
        assert!(view.wsl_paste.is_some());
        assert!(view.local_error.is_none(), "{:?}", view.local_error);
        assert!(!view.flush_pending_input(cx));
        close_gap(view);
        // Polling delivers the paths first, then the key typed after them.
        assert!(view.flush_pending_input(cx));
        assert!(view.wsl_paste.is_none());
        assert!(view.local_error.is_none(), "{:?}", view.local_error);
    });
    assert_eq!(
        peer.receive(),
        pane_input(vec![ClientPaneInputEvent::Paste("/mnt/c/a".into())])
    );
    assert_eq!(
        peer.receive(),
        pane_input(vec![ClientPaneInputEvent::TextCommit("x".into())])
    );
}

#[gpui::test]
fn paths_stop_waiting_for_a_surface_that_never_comes(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = mapping(cx);
    view.update(cx, |view, cx| {
        open_gap(view);
        finish(view, &["/mnt/c/a"], cx);
        let (_, mapped_at) = view.wsl_paste.as_mut().unwrap().mapped.as_mut().unwrap();
        *mapped_at -= GAP_LIMIT;
        assert!(!view.flush_pending_input(cx));
        assert!(view.wsl_paste.is_none());
        assert!(view.local_error.is_some());
        close_gap(view);
        sentinel(view, &mut peer);
    });
}

#[gpui::test]
fn paths_waiting_in_a_gap_are_refused_once_the_connection_drops(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = mapping(cx);
    view.update(cx, |view, cx| {
        open_gap(view);
        finish(view, &["/mnt/c/a"], cx);
        assert!(view.wsl_paste.is_some());
        view.live.status = crate::state::ConnectionStatus::Disconnected;
        assert!(!view.flush_pending_input(cx));
        assert!(view.wsl_paste.is_none());
        assert_eq!(
            view.local_error.as_deref(),
            Some(
                "Files not pasted: The pane the files were dropped on changed before their paths were ready."
            )
        );
        view.live.status = crate::state::ConnectionStatus::Connected;
        close_gap(view);
        sentinel(view, &mut peer);
    });
}
