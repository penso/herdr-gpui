//! The `en-XA` leak test (`crate::i18n::leak_tests`) for a saved device's
//! menu and its pages: each draws only translated messages, except for the
//! device's name, target, and session, and theme names.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

/// Opens the saved host's menu, then the page `action` leads to.
fn open(
    view: &gpui::Entity<crate::HerdrWindow>,
    cx: &mut VisualTestContext,
    action: Option<Action>,
) {
    view.update_in(cx, |view, window, cx| {
        view.dismiss_menu(window, cx);
        view.open_host_menu(HOST, point(px(20.), px(20.)), window, cx);
        if let Some(action) = action {
            view.activate_host_menu(action, window, cx);
        }
    });
    draw(cx);
}

#[test]
fn the_host_menu_and_its_pages_show_only_messages_under_en_xa() {
    under_en_xa("host menu en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = fixture_window(window, cx);
            add_host(&mut view);
            view
        });
        cx.simulate_resize(size(px(1000.), px(800.)));
        draw(cx);
        recorder.take();
        // The header truncates the device's name and target, so their
        // characters are measured one at a time.
        let names = ["m5max-ms", "penso@box · default"];
        let device: Vec<String> = names
            .iter()
            .map(|name| (*name).to_owned())
            .chain(names.iter().flat_map(|name| name.chars().map(String::from)))
            .collect();

        open(&view, cx, None);
        assert_translated("Host menu", &recorder.take(), &device);

        open(&view, cx, Some(Action::Rename));
        assert_translated("Host menu > Rename", &recorder.take(), &device);

        open(&view, cx, Some(Action::Remove));
        assert_translated("Host menu > Remove", &recorder.take(), &device);

        #[cfg(unix)]
        {
            open(&view, cx, Some(Action::ForwardPort));
            assert_translated(
                "Host menu > Forward port",
                &recorder.take(),
                &["3000".into()],
            );
        }

        open(&view, cx, Some(Action::Theme));
        let themes: Vec<String> = cx.update(|_, cx| {
            view.read(cx)
                .menu
                .host
                .as_ref()
                .and_then(|host| host.theme.as_ref())
                .map(|picker| {
                    picker
                        .filtered
                        .iter()
                        .filter_map(|row| match row {
                            super::super::theme::Row::Named(name) => Some(name.clone()),
                            super::super::theme::Row::App => None,
                        })
                        .collect()
                })
                .unwrap_or_default()
        });
        assert_translated("Host menu > Terminal theme", &recorder.take(), &themes);
    });
}

#[test]
fn every_action_names_a_message() {
    #[cfg(feature = "cloud")]
    let actions = SSH_ACTIONS.iter().chain(super::super::CLOUD_ACTIONS);
    #[cfg(not(feature = "cloud"))]
    let actions = SSH_ACTIONS.iter();
    for &(action, message) in actions {
        assert_eq!(
            crate::i18n::takes(message),
            Some(&[][..]),
            "{action:?} names {message}"
        );
    }
}
