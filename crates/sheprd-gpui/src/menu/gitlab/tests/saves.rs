//! Saving a pasted token: it is checked where the listed instance is read
//! from, and the field is emptied only while it still holds that token.
use super::*;
use secrecy::SecretString;

#[gpui::test]
fn a_token_is_checked_where_its_remotes_read_the_instance(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    // glab sends this instance's API over HTTP, but its remote reads it over
    // HTTPS, so that is where the token is checked as well.
    let cli = [CliHost {
        host: "gitlab.example.com".into(),
        api_protocol: Some("http".into()),
        ..CliHost::default()
    }];
    let hosts = merge_hosts(
        [read("gitlab.example.com", Scheme::Https, None)],
        ["gitlab.example.com".to_owned()],
        &cli,
    );
    view.update(cx, |view, _| {
        view.show_gitlab_hosts(&hosts, None, || Token::Unknown);
        let api = &view.menu.gitlab.apis["gitlab.example.com"];
        assert_eq!(
            (api.host.as_str(), api.scheme),
            ("gitlab.example.com", Scheme::Https)
        );
    });
}

#[gpui::test]
fn a_finished_save_keeps_a_token_typed_after_it(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    for (typed, cleared) in [("glpat-next", false), ("glpat-saved", true)] {
        type_in(&view, cx, "gitlab.com", typed);
        view.update(cx, |view, cx| {
            let saved = Some(SecretString::from("glpat-saved"));
            view.run_gitlab(cx, saved, || Ok("Saved.".to_owned()));
        });
        draw(cx);
        let (message, token) = message_and_token(&view, cx);
        assert_eq!(message, "Saved.");
        assert_eq!(token.is_empty(), cleared, "{typed}");
    }
}

#[gpui::test]
fn a_token_pasted_while_connect_waits_stops_that_wait(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    for (waiting, stopped) in [("gitlab.com", true), ("gitlab.example.com", false)] {
        view.update(cx, |view, _| {
            view.menu.gitlab.connecting = Some(super::super::connect::Connecting::fixture(
                waiting,
                "ABCD-1234",
            ));
        });
        type_in(&view, cx, "gitlab.com", "glpat-pasted");
        view.update(cx, |view, cx| view.save_gitlab(cx));
        let waits = view.read_with(cx, |view, _| view.menu.gitlab.connecting.is_some());
        assert_eq!(waits, !stopped, "{waiting}");
    }
}
