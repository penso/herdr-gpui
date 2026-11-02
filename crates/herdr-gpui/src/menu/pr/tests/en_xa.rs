//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the workspace
//! popover's pull request section: its card, and the lines that say why
//! there is none. Each draws only translated messages, except for what
//! GitHub reports. The rest of the popover is drawn first and not checked
//! here.
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
use gpui::{point, px, size};

/// `names` as a narrow row may show them: whole, cut short at any point
/// and ended with "…", and the single letters shaped to measure the cut.
fn cut(names: impl IntoIterator<Item = String>) -> Vec<String> {
    names
        .into_iter()
        .flat_map(|name| {
            let cuts = name
                .char_indices()
                .skip(1)
                .map(|(at, _)| format!("{}\u{2026}", name[..at].trim_end()))
                .collect::<Vec<_>>();
            let letters = name.chars().map(String::from).collect::<Vec<_>>();
            std::iter::once(name).chain(cuts).chain(letters)
        })
        .collect()
}

#[test]
fn the_pull_request_section_shows_only_messages_under_en_xa() {
    under_en_xa("workspace pull request en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(1200.), px(800.)));
        view.update_in(cx, |view, window, cx| {
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.open_workspace_menu("w3", point(px(620.), px(380.)), window, cx);
            view.menu.github = crate::github::Auth::connected_fixture();
            view.menu.pr.clear();
            view.menu.pr.loading = true;
            cx.notify();
        });
        draw(cx);
        recorder.take();

        let pr = crate::pull_request::fixture().unwrap();
        let data = cut([
            format!("#{} {}", pr.number, pr.title),
            format!("{} -> {}", pr.head_ref_name, pr.base_ref_name),
            pr.lifecycle().to_owned(),
            pr.checks_summary.clone(),
            pr.review().to_owned(),
            pr.merge_status().to_owned(),
        ]);
        view.update_in(cx, |view, _, cx| {
            view.menu.pr.loading = false;
            view.menu.pr.value = Some(pr);
            cx.notify();
        });
        draw(cx);
        assert_translated("Workspace > pull request", &recorder.take(), &data);

        view.update_in(cx, |view, _, cx| {
            view.menu.pr.message = Some("offline".into());
            cx.notify();
        });
        draw(cx);
        assert_translated("Workspace > stale pull request", &recorder.take(), &data);

        view.update_in(cx, |view, _, cx| {
            view.menu.pr.clear();
            cx.notify();
        });
        draw(cx);
        assert_translated("Workspace > no pull request", &recorder.take(), &data);
    });
}
