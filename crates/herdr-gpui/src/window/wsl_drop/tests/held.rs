//! A WSL drop held across a frame gap keeps the keys held after it behind
//! its paths, and a held drop never reaches a pane under an open menu.

use super::*;
use crate::{menu::Page, window::pending_input::DropAt};

/// A connected WSL window whose frame another client sized, so input is
/// held. The distribution's name is refused before `wsl.exe` would start.
fn held(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, MockPeer, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| Fixture {
        view: cx.new(|cx| fixture_window(window, cx)),
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.view.clone());
    let peer = MockPeer::new();
    view.update(cx, |view, cx| {
        peer.prepare(view, cx);
        view.endpoints[0].connection.target = ConnectTarget::Wsl {
            distro: "a;b".into(),
            session: "default".into(),
        };
        assert!(view.flush_pending_input(cx));
        view.options.surface_size.cols += 1;
        assert!(!view.input_ready());
    });
    (view, peer, cx)
}

fn hold_drop_then_enter(view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>) {
    let at = DropAt::Pane(InputTarget::Pane(PANE.into()));
    assert!(view.hold_drop(at, vec![PathBuf::from("C:/a")], cx));
    view.send(crate::menu::enter_key(), cx);
    assert_eq!(view.pending_input.len(), 2);
}

fn frame_returns(view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>) {
    view.options.surface_size.cols -= 1;
    assert!(view.flush_pending_input(cx));
}

#[gpui::test]
fn a_key_held_after_a_wsl_drop_follows_its_paths(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = held(cx);
    view.update(cx, |view, cx| {
        hold_drop_then_enter(view, cx);
        frame_returns(view, cx);
        assert!(view.wsl_paste.is_some(), "the paths are being mapped");
        assert_eq!(view.pending_input.len(), 1, "Enter waits for them");
        finish(view, &["/mnt/c/a"], cx);
        assert!(view.local_error.is_none(), "{:?}", view.local_error);
    });
    let mut next = || loop {
        let message = peer.receive();
        if matches!(message, ClientMessage::ClientShellPaneInput { .. }) {
            return message;
        }
    };
    assert_eq!(
        next(),
        pane_input(vec![ClientPaneInputEvent::Paste("/mnt/c/a".into())])
    );
    assert_eq!(next(), pane_input(vec![crate::menu::enter_key()]));
}

#[gpui::test]
fn a_key_held_after_a_wsl_drop_whose_mapping_fails_is_not_sent(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = held(cx);
    view.update(cx, |view, cx| {
        hold_drop_then_enter(view, cx);
        frame_returns(view, cx);
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.wsl_paste.is_none());
        assert_eq!(view.pending_input.len(), 0);
        assert_eq!(
            view.local_error.as_deref(),
            Some("Files not pasted: invalid WSL distribution name (1 held input discarded)")
        );
        sentinel(view, &mut peer);
    });
}

#[gpui::test]
fn a_key_held_after_a_drop_refused_by_an_open_menu_is_not_sent(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = held(cx);
    view.update(cx, |view, cx| {
        hold_drop_then_enter(view, cx);
        view.menu.page = Some(Page::About);
        frame_returns(view, cx);
        assert!(view.wsl_paste.is_none(), "nothing was mapped");
        assert_eq!(
            view.local_error.as_deref(),
            Some("Files not pasted: a menu is open (1 held input discarded)")
        );
        view.menu.page = None;
        sentinel(view, &mut peer);
    });
}
