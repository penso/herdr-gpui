//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the Git popup's
//! pull request dialogs: checks and comments, a comment, and merging. Each
//! draws only translated messages, except for what GitHub reports.
use super::*;
use crate::i18n::leak_tests::{assert_translated, under_en_xa};

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

/// What GitHub reports of `pr`, which the dialogs show as it is.
fn reported(pr: &crate::pull_request::PullRequest) -> Vec<String> {
    let mut data = vec![
        pr.title.clone(),
        format!("#{} {}", pr.number, pr.title),
        pr.lifecycle().to_owned(),
        pr.review().to_owned(),
        pr.merge_status().to_owned(),
        pr.checks_summary.clone(),
        pr.head_ref_name.clone(),
        pr.base_ref_name.clone(),
    ];
    for (name, outcome) in pr.check_list() {
        data.push(name.to_owned());
        data.push(outcome.to_string());
    }
    data.extend(
        pr.merge_methods
            .iter()
            .map(|method| method.action().to_owned()),
    );
    cut(data)
}

#[test]
fn the_pull_request_dialogs_show_only_messages_under_en_xa() {
    under_en_xa("pull request dialogs en-XA leak", |cx, recorder| {
        let pr = crate::pull_request::fixture().unwrap();
        let data = reported(&pr);
        let (view, cx) = open(cx, pr);
        draw(cx);
        for (row, surface) in [
            (Row::Review, "Git > Checks and comments"),
            (Row::Comment, "Git > Comment"),
            (Row::Merge, "Git > Merge"),
        ] {
            recorder.take();
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.menu.page = Some(Page::Git);
                    view.activate_git_row(row, window, cx);
                })
            });
            draw(cx);
            assert_translated(surface, &recorder.take(), &data);
        }
    });
}
