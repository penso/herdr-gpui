//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the account
//! menu's GitHub page: signing in with a code, a pasted token, and an
//! account whose GitHub App waits on the user. Each draws only translated
//! messages, except for logins, the code, and what `herdr-github` reports.
use crate::github::{Access, Auth, Installation, Note};
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
use crate::menu::Page;

/// What the page shows as it is: the tabs' product names, GitHub's device
/// page, and what `herdr-github` says of the session and its store.
fn reported(view: &crate::HerdrWindow) -> Vec<String> {
    let auth = &view.menu.github;
    let note = crate::github::store_note(auth.store(), auth.connected()).map(|note| match note {
        Note::Warning(text) | Note::Info(text) => text.to_string(),
    });
    let login = auth
        .profile
        .as_ref()
        .map(|profile| format!("@{}", profile.login));
    ["GitHub", "GitLab", "github.com/login/device", "ABCD-1234"]
        .map(str::to_owned)
        .into_iter()
        .chain(auth.message.clone())
        .chain(note)
        .chain(login)
        .collect()
}

fn show(
    view: &gpui::Entity<crate::HerdrWindow>,
    cx: &mut gpui::VisualTestContext,
    auth: Auth,
    token: bool,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.dismiss_menu(window, cx);
            view.open_menu(window, cx);
            view.menu.page = Some(Page::GitHub);
            view.menu.github = auth;
            if token {
                view.open_github_token(window, cx);
            }
            cx.notify();
        })
    });
    draw(cx);
}

#[test]
fn the_github_page_shows_only_messages_under_en_xa() {
    under_en_xa("GitHub page en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(gpui::size(gpui::px(1000.), gpui::px(900.)));
        draw(cx);
        let pending = Access::Pending(
            (1..=7)
                .map(|id| Installation::fixture(&format!("org{id}"), true, id))
                .collect(),
        );
        let mut waiting = Auth::connected_fixture();
        waiting.profile.as_mut().unwrap().access = pending;
        for (surface, auth, token) in [
            ("GitHub > code", Auth::fixture(true), false),
            ("GitHub > signed out", Auth::fixture(false), false),
            ("GitHub > token", Auth::default(), true),
            ("GitHub > app approval", waiting, false),
        ] {
            recorder.take();
            show(&view, cx, auth, token);
            let data = view.read_with(cx, |view, _| reported(view));
            assert_translated(surface, &recorder.take(), &data);
        }
    });
}
