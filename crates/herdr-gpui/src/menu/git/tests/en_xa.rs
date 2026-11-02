//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the title bar's
//! Git popup and its commit dialog: each draws only translated messages,
//! except for the branch and what GitHub reports of its pull request.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

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

fn input() -> crate::pull_request::Input {
    crate::pull_request::Input {
        checkout: None,
        repo_key: Some(REPO_KEY.into()),
        branch: "develop".into(),
    }
}

#[test]
fn the_git_popup_and_commit_dialog_show_only_messages_under_en_xa() {
    under_en_xa("Git popup en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(900.), px(700.)));
        draw(cx);
        recorder.take();
        let pr = crate::pull_request::fixture().unwrap();
        let data = cut([
            // The title bar's Git chip names the repository.
            "agent-launcher".to_owned(),
            "develop".to_owned(),
            pr.title.clone(),
            pr.lifecycle().to_owned(),
            pr.review().to_owned(),
            pr.merge_status().to_owned(),
            pr.checks_summary.clone(),
        ]);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.git = crate::git::Git::fixture(input(), status(12, 3, 2));
                view.menu.github = crate::github::Auth::connected_fixture();
                view.seed_pr(input(), pr, std::time::Instant::now());
                view.pr_actions.track(
                    view.git_open_pull_request()
                        .and_then(|pr| crate::pr_actions::Target::try_from(&*pr).ok()),
                );
                view.open_git_menu(point(px(860.), px(20.)), window, cx);
            })
        });
        draw(cx);
        assert_translated("Git", &recorder.take(), &data);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.activate_git_row(Row::Commit, window, cx)
            })
        });
        draw(cx);
        assert_translated("Git > Commit", &recorder.take(), &data);
    });
}
