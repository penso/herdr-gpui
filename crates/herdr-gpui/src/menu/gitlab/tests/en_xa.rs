//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the account
//! menu's GitLab page: the welcome page while connecting, the instances'
//! cards, and the pasted-token form. Each draws only translated messages,
//! except for instances, usernames, the code, and token sources by name.
use super::*;
use crate::i18n::leak_tests::{assert_translated, under_en_xa};
use crate::menu::gitlab::connect::Connecting;

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

/// What the page shows as it is: the tabs' product names, the instances,
/// whom they sign in as, token sources named by file, variable, or
/// program, the fields' placeholders, and GitLab's code.
fn reported() -> Vec<String> {
    cut([
        "GitHub",
        "GitLab",
        "gitlab.com",
        "gitlab.example.com",
        "@akanoa",
        ".envrc",
        "GITLAB_TOKEN",
        "glab",
        "glpat-\u{2026}",
        "WXYZ-9876",
    ]
    .map(str::to_owned))
}

#[test]
fn the_gitlab_page_shows_only_messages_under_en_xa() {
    under_en_xa("GitLab page en-XA leak", |cx, recorder| {
        let (view, cx) = page(cx);
        recorder.take();
        view.update_in(cx, |view, _, cx| {
            let gitlab = &mut view.menu.gitlab;
            gitlab.hosts = vec![
                HostStatus {
                    can_connect: true,
                    checkout: Some("app".into()),
                    ..status(None, None)
                },
                HostStatus {
                    host: "gitlab.example.com".into(),
                    ..status(None, None)
                },
            ];
            gitlab.connecting = Some(Connecting::fixture("gitlab.com", "WXYZ-9876"));
            cx.notify();
        });
        draw(cx);
        assert_translated("GitLab > welcome", &recorder.take(), &reported());

        view.update_in(cx, |view, _, cx| {
            let gitlab = &mut view.menu.gitlab;
            gitlab.connecting = None;
            gitlab.hosts = [
                (Some(Source::Saved), Some(Ok("akanoa"))),
                (Some(Source::Envrc), None),
                (
                    Some(Source::Environment),
                    Some(Err(gitlab::Error::Authentication)),
                ),
                (Some(Source::Cli), Some(Err(gitlab::Error::EnvrcRefused))),
            ]
            .into_iter()
            .map(|(source, user)| status(source, user))
            .chain([HostStatus {
                host: "gitlab.example.com".into(),
                can_connect: true,
                ..status(None, None)
            }])
            .collect();
            cx.notify();
        });
        draw(cx);
        assert_translated("GitLab > instances", &recorder.take(), &reported());

        view.update_in(cx, |view, window, cx| {
            view.show_gitlab_form(true, window, cx)
        });
        draw(cx);
        assert_translated("GitLab > token", &recorder.take(), &reported());
    });
}
