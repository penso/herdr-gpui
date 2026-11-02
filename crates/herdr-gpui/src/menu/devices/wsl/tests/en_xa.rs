//! The `en-XA` leak test (`crate::i18n::leak_tests`) for adding and
//! removing a WSL distribution: each page draws only translated messages,
//! except for the distributions' names.
use super::*;
use crate::i18n::{
    fl,
    leak_tests::{assert_translated, draw, under_en_xa},
};
use gpui::{px, size};

#[test]
fn adding_and_removing_a_distribution_shows_only_messages_under_en_xa() {
    under_en_xa("WSL en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(1000.), px(800.)));
        draw(cx);
        recorder.take();
        let distros = vec!["Ubuntu".to_owned(), "Debian".to_owned()];
        let data: Vec<String> = distros
            .iter()
            .cloned()
            .chain(["WSL".into(), "default".into()])
            .collect();

        view.update(cx, |view, cx| {
            open_form(view, &["Ubuntu", "Debian"], cx);
            // As the form opens it, with Herdr's default session.
            let session = view.menu.wsl_setup.as_ref().unwrap().session.clone();
            session.update(cx, |input, cx| input.set_placeholder("default", cx));
        });
        draw(cx);
        assert_translated("Add WSL", &recorder.take(), &data);

        let cases = [
            ("looking", Distros::Loading, Step::Form, None),
            (
                "none installed",
                Distros::Ready(Vec::new()),
                Step::Form,
                None,
            ),
            (
                "list failed",
                Distros::Failed("access denied".into()),
                Step::Form,
                None,
            ),
            (
                "checking",
                Distros::Ready(distros.clone()),
                Step::Checking("Ubuntu".into()),
                Some(fl!("devices-wsl-missing", distro = "Ubuntu")),
            ),
        ];
        for (name, distros, step, error) in cases {
            view.update(cx, |view, cx| {
                if let Some(setup) = &mut view.menu.wsl_setup {
                    setup.distros = distros;
                    setup.step = step;
                    setup.error = error;
                }
                cx.notify();
            });
            draw(cx);
            assert_translated(&format!("Add WSL ({name})"), &recorder.take(), &data);
        }

        view.update_in(cx, |view, window, cx| {
            view.dismiss_menu(window, cx);
            let id = format!("{}Ubuntu", crate::endpoint::WSL_PREFIX);
            view.open_remove_wsl(&id, Point::default(), window, cx);
        });
        draw(cx);
        assert_translated("Remove WSL", &recorder.take(), &data);
    });
}
