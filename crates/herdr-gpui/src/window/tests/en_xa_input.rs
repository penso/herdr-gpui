//! The `en-XA` leak test for what the main window says about input it did
//! not deliver: the flash for input typed while disconnected, and the status
//! line's refused pastes and file drops, apart from the snapshot's names.
use super::en_xa::check;
use super::*;
use crate::i18n::leak_tests::under_en_xa;
use crate::state::ConnectionStatus;
use crate::window::pending_input::DropAt;
use gpui::{point, px, size};
use herdr_client::protocol::ClientPaneInputEvent;

#[test]
fn undelivered_input_draws_only_messages() {
    under_en_xa("window input en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = fixture_window(window, cx);
            view.live.snapshot = Some(Arc::new(snapshot(2)));
            view.live.status = ConnectionStatus::Disconnected;
            view.endpoints[0].live = view.live.clone();
            view
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        view.update(cx, |view, cx| {
            assert!(!view.send(ClientPaneInputEvent::Paste("typed".into()), cx));
        });
        check("Input while disconnected", &view, cx, recorder);

        view.update(cx, |view, cx| {
            view.live.status = ConnectionStatus::Connected;
            view.endpoints[0].live = view.live.clone();
            view.menu.page = Some(crate::menu::Page::AgentSkill);
            assert!(view.refuse_paste(cx).is_some());
            view.menu.page = None;
            cx.notify();
        });
        check("Paste refused", &view, cx, recorder);

        view.update(cx, |view, cx| {
            view.menu.page = Some(crate::menu::Page::AgentSkill);
            view.deliver_held_drop(DropAt::Point(point(px(1.), px(1.))), &[], cx);
            view.menu.page = None;
            cx.notify();
        });
        check("Drop refused", &view, cx, recorder);

        view.update(cx, |view, cx| {
            // Far outside the window, where no pane is.
            view.deliver_held_drop(DropAt::Point(point(px(-500.), px(-500.))), &[], cx);
        });
        check("Drop on nothing", &view, cx, recorder);
    });
}
