//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the Ports section
//! and its confirmation: each draws only translated messages, except for
//! the processes' names and addresses.
use super::*;
use crate::{
    i18n::leak_tests::{assert_translated, draw, under_en_xa},
    sidebar::ActivityPanel,
};
use gpui::{px, size};

#[test]
fn the_ports_section_and_its_question_show_only_messages_under_en_xa() {
    under_en_xa("ports en-XA leak", |cx, recorder| {
        let (ports, _updates, _commands) = LocalPorts::fake();
        let mut paused = listener(8080, 4343, 20);
        paused.paused = true;
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view.local_ports = ports;
            view.local_ports
                .seed(vec![listener(3000, 4242, 10), paused.clone()]);
            view
        });
        cx.simulate_resize(size(px(1100.), px(700.)));
        draw(cx);
        recorder.take();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.show_activity_panel(ActivityPanel::Ports, window, cx)
            })
        });
        cx.update(|window, _| window.refresh());
        draw(cx);
        let data = [
            "node".to_owned(),
            "vite".to_owned(),
            "node /home/dev/app/node_modules/.bin/vite".to_owned(),
            "localhost:3000".to_owned(),
            "localhost:8080".to_owned(),
            // Truncating a line measures its first character alone.
            "v".to_owned(),
            "n".to_owned(),
        ];
        assert_translated("Ports", &recorder.take(), &data);

        recorder.take();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.ask_to_signal_ports(
                    Pending {
                        action: PortAction::Stop,
                        listeners: vec![paused.clone()],
                    },
                    window,
                    cx,
                );
            })
        });
        draw(cx);
        assert_translated("Ports > Stop", &recorder.take(), &data);

        // A failed scan says why in the app's language, not in English.
        recorder.take();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.dismiss_menu(window, cx);
                view.local_ports.seed_error(Error::Tool.into());
                cx.notify();
            })
        });
        cx.update(|window, _| window.refresh());
        draw(cx);
        let shown = recorder.take();
        assert!(cx.debug_bounds("local-ports-error").is_some());
        let english = Error::Tool.to_string();
        assert!(
            !shown.iter().any(|line| line.contains(english.as_str())),
            "the failure is shown in English: {shown:?}"
        );
        assert_translated("Ports > failed scan", &shown, &data);
    });
}
