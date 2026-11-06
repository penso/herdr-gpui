//! Looking the instances up again keeps each card as it was, picture
//! included, until the new answer lands, and a connected account GitLab
//! refuses can be connected again from its card.
use super::*;

#[gpui::test]
fn a_new_check_keeps_each_card_and_its_picture_until_it_lands(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    let hosts = merge_hosts([read("gitlab.com", Scheme::Https, None)], [], &[]);
    let avatar = "https://gitlab.com/uploads/-/system/user/avatar/1/avatar.png";
    view.update(cx, |view, _| {
        let mut signed_in = status(Some(Source::Saved), Some(Ok("Akanoa")));
        if let Token::Found {
            user: Some(Ok(user)),
            ..
        } = &mut signed_in.token
        {
            user.avatar_url = Some(avatar.into());
        }
        view.menu.gitlab.hosts = vec![signed_in];
        view.show_gitlab_hosts(&hosts, None, || Token::Unknown);
        let gitlab = &view.menu.gitlab;
        assert!(gitlab.checking.contains("gitlab.com"));
        assert!(matches!(
            &gitlab.hosts[0].token,
            Token::Found { user: Some(Ok(user)), .. }
                if user.avatar_url.as_deref() == Some(avatar)
        ));
        // The menu closing stops the check and leaves the card as it was.
        view.menu.gitlab.stop_lookups();
        assert!(view.menu.gitlab.checking.is_empty());
        assert!(matches!(
            view.menu.gitlab.hosts[0].token,
            Token::Found { .. }
        ));
    });
}

#[gpui::test]
fn a_connected_account_gitlab_refuses_can_connect_again(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    view.update(cx, |view, cx| {
        view.menu.gitlab.hosts = vec![HostStatus {
            connected: true,
            can_connect: true,
            ..status(
                Some(Source::Saved),
                Some(Err(gitlab::Error::Authentication)),
            )
        }];
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("gitlab-connect-0").is_some());
    assert!(cx.debug_bounds("gitlab-forget-0").is_some());
}
