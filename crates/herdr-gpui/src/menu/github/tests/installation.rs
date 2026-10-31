use super::*;
use crate::github::{Access, Auth, Installation};
use crate::menu::github::installation::attention;

const INSTALL: &str = "https://github.com/apps/herdr-ui/installations/new";

fn pending(count: usize) -> Access {
    Access::Pending(
        std::iter::once(Installation::fixture("me", false, 1))
            .chain(
                (1..count as u64)
                    .map(|id| Installation::fixture(&format!("org{id}"), true, id + 1)),
            )
            .collect(),
    )
}

fn waiting(access: Access) -> Auth {
    let mut auth = Auth::connected_fixture();
    auth.profile.as_mut().unwrap().access = access;
    auth
}

#[test]
fn nothing_is_said_while_access_is_ready_or_unknown() {
    assert_eq!(attention("me", &Access::Ready, Some(INSTALL)), None);
    assert_eq!(attention("me", &Access::Unknown, Some(INSTALL)), None);
}

#[test]
fn an_uninstalled_app_links_its_install_page_only_when_known() {
    let known = attention("me", &Access::NotInstalled, Some(INSTALL)).unwrap();
    assert_eq!(
        known.links,
        [("Install on GitHub".to_owned(), INSTALL.to_owned())]
    );
    assert!(known.body.contains("@\u{2068}me\u{2069}"));
    // Another App's page cannot be found from its client ID.
    let custom = attention("me", &Access::NotInstalled, None).unwrap();
    assert!(custom.links.is_empty());
    assert!(custom.body.contains("oauth_client_id"));
}

#[test]
fn pending_owners_are_each_linked_up_to_a_bound() {
    let one = attention("me", &pending(1), None).unwrap();
    assert_eq!(
        one.links,
        [(
            "Approve for @\u{2068}me\u{2069}".to_owned(),
            "https://github.com/settings/installations/1".to_owned()
        )]
    );
    assert!(one.body.contains("it is approved"));
    let many = attention("me", &pending(8), None).unwrap();
    assert_eq!(many.links.len(), 5);
    assert_eq!(many.unlisted, 3);
    assert_eq!(
        many.links[1],
        (
            "Approve for \u{2068}org1\u{2069} (needs an owner of it)".to_owned(),
            "https://github.com/organizations/org1/settings/installations/2".to_owned()
        )
    );
}

#[gpui::test]
fn a_waiting_account_marks_the_avatar_and_offers_approval(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    };
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.menu.github = Auth::connected_fixture());
    });
    draw(cx);
    assert!(cx.debug_bounds("titlebar-avatar").is_some());
    assert!(cx.debug_bounds("titlebar-avatar-attention").is_none());

    cx.update(|_, cx| {
        view.update(cx, |view, _| view.menu.github = waiting(pending(2)));
    });
    draw(cx);
    let avatar = cx.debug_bounds("titlebar-avatar").unwrap();
    let dot = cx.debug_bounds("titlebar-avatar-attention").unwrap();
    // The dot stays inside the avatar's hit target.
    assert!(avatar.contains(&dot.origin) && avatar.contains(&dot.bottom_right()));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_menu(window, cx);
            view.menu.page = Some(crate::menu::Page::GitHub);
            assert!(view.github_actions().contains(&Action::Approve));
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("github-attention").is_some());
    assert!(cx.debug_bounds("github-approve").is_some());
    assert!(cx.debug_bounds("github-attention-link-1").is_some());

    // A custom App with no install page has nothing for the button to open.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.config.github.oauth_client_id = Some("Iv1.custom".into());
            view.menu.github = waiting(Access::NotInstalled);
            assert!(!view.github_actions().contains(&Action::Approve));
        });
    });
}

#[gpui::test]
fn a_waiting_account_is_toasted_once_per_window(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let toasts = |view: &crate::HerdrWindow| {
                view.endpoints[view.selected_endpoint].toasts.entries.len()
            };
            let before = toasts(view);
            view.menu.github = Auth::connected_fixture();
            view.toast_github_attention(cx);
            assert_eq!(toasts(view), before);

            view.menu.github = waiting(Access::NotInstalled);
            view.toast_github_attention(cx);
            assert_eq!(toasts(view), before + 1);
            let notice = &view.endpoints[view.selected_endpoint]
                .toasts
                .entries
                .back()
                .unwrap()
                .1;
            assert_eq!(notice.title, "Install the GitHub App");

            // Later checks finding the same account waiting stay quiet.
            view.menu.github = waiting(pending(1));
            view.toast_github_attention(cx);
            assert_eq!(toasts(view), before + 1);
        });
    });
}

#[gpui::test]
fn one_login_in_several_account_slots_is_toasted_once(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let toasts = |view: &crate::HerdrWindow| {
                view.endpoints[view.selected_endpoint].toasts.entries.len()
            };
            let before = toasts(view);
            // The device's account and a saved SSH device's, same login,
            // both found waiting by one poll.
            view.menu.github = waiting(Access::NotInstalled);
            view.menu
                .github_hosts
                .insert("devbox".into(), waiting(pending(1)));
            view.toast_github_attention(cx);
            assert_eq!(toasts(view), before + 1);
        });
    });
}
