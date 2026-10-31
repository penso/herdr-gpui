//! The `en-XA` leak test for the main window's notices: a toast, a flash,
//! the config diagnostic card, the agent skill offer, and an error on the
//! status line draw only translated messages, apart from the snapshot's
//! names and the config's own diagnostic text.
use super::en_xa::{check, data};
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa};
use crate::state::ConnectionStatus;
use crate::window::Flash;
use gpui::{px, size};
use herdr_client::protocol::SemanticNotificationKind;

#[test]
fn the_main_windows_notices_draw_only_messages() {
    under_en_xa("main window notices en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = fixture_window(window, cx);
            view.live.snapshot = Some(Arc::new(snapshot(2)));
            view.live.status = ConnectionStatus::Connected;
            view.endpoints[0].live = view.live.clone();
            view
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        check("Window", &view, cx, recorder);

        for kind in [
            SemanticNotificationKind::NeedsAttention,
            SemanticNotificationKind::Finished,
            SemanticNotificationKind::UpdateInstalled,
            SemanticNotificationKind::Custom,
        ] {
            view.update(cx, |view, cx| view.show_toast_preview(kind, cx));
            check(&format!("Toast {kind:?}"), &view, cx, recorder);
            view.update(cx, |view, cx| {
                view.endpoints[0].toasts.entries.clear();
                cx.notify();
            });
        }

        view.update(cx, |view, cx| view.hint_shift_select(cx));
        check("Shift-select hint", &view, cx, recorder);
        view.update(cx, |view, cx| {
            view.show_flash(
                Flash::warning(crate::i18n::fl!("window-notices-move-tabs-needs-newer")),
                cx,
            );
        });
        check("Flash", &view, cx, recorder);

        view.update(cx, |view, cx| {
            view.local_error = Some(crate::i18n::fl!(
                "window-notices-request-failed",
                operation = crate::i18n::fl!("window-notices-navigate"),
                error = crate::i18n::Localize::localize(&crate::Error::NotConnected)
            ));
            cx.notify();
        });
        check("Status error", &view, cx, recorder);

        // The config's diagnostic is the config module's text, not this
        // card's: only the card's own label must be a message.
        let config = crate::config::Config {
            unknown_keys: vec!["future.key".into()],
            ..Default::default()
        };
        let diagnostic = config.diagnostic().unwrap();
        view.update(cx, |view, cx| {
            view.gui_config_diagnostic.sync(Some(&diagnostic));
            cx.notify();
        });
        cx.update(|window, _| window.refresh());
        draw(cx);
        // The card cuts its lines short to fit.
        let shown = uncut(recorder.take());
        let mut known = cx.update(|_, cx| data(view.read(cx), &shown));
        known.extend(diagnostic.lines().map(str::to_owned));
        assert_translated("Config diagnostic", &shown, &known);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                if view.open_menu(window, cx) {
                    view.menu.page = Some(crate::menu::Page::AgentSkill);
                    cx.notify();
                }
            });
        });
        check("Agent skill offer", &view, cx, recorder);
    });
}
