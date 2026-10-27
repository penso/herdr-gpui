#![allow(clippy::unwrap_used)]

use super::*;
use crate::{sidebar::layout_tests::fixture_window, window::MockPeer};
use core::prelude::v1::test;
use gpui::accesskit::{Action, Node};
use herdr_client::protocol::{ClientMessage, ClientShellPopupSurface};
use std::sync::Arc;

mod insertion;

fn prepare(
    peer: &MockPeer,
    view: &mut HerdrWindow,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) {
    peer.prepare(view, cx);
    window.focus(&view.focus, cx);
    assert!(view.flush_pending_input(cx));
}

fn node(view: &HerdrWindow, window: &Window, cx: &Context<HerdrWindow>) -> Node {
    let element = view
        .accessible_terminal(div().id("terminal"), window, cx)
        .into_element();
    let mut node = Node::new(element.a11y_role().unwrap());
    element.write_a11y_info(&mut node);
    node
}

fn value(text: &str) -> ActionData {
    ActionData::Value(text.into())
}

fn next_input(peer: &mut MockPeer) -> ClientMessage {
    loop {
        let message = peer.receive();
        if matches!(
            message,
            ClientMessage::ClientShellPaneInput { .. }
                | ClientMessage::ClientShellPopupInput { .. }
        ) {
            return message;
        }
    }
}

fn pane_input(text: &str) -> ClientMessage {
    ClientMessage::ClientShellPaneInput {
        pane_id: "w1:p1".into(),
        events: vec![ClientPaneInputEvent::Paste(text.into())],
    }
}

fn assert_no_input(peer: &mut MockPeer) {
    peer.client
        .handle
        .send_input(
            "boot-v1",
            "w1:p1",
            [ClientPaneInputEvent::Paste("sentinel".into())],
        )
        .unwrap();
    assert_eq!(next_input(peer), pane_input("sentinel"));
}

fn popup(view: &mut HerdrWindow) {
    let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
    surface.popup = Some(Box::new(ClientShellPopupSurface {
        terminal_id: "popup".into(),
        title: "Popup".into(),
        width: None,
        height: None,
        frame: surface.frame.clone(),
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        pixel_width: 800,
        pixel_height: 480,
    }));
}
