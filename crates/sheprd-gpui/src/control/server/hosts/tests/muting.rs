use super::*;
use crate::notification_mutes::Mutes;

fn muted(cx: &App, id: &str) -> bool {
    Mutes::of(cx).is_some_and(|mutes| mutes.host(id))
}

#[gpui::test]
fn a_host_mutes_by_label_or_as_the_callers_own_and_says_when_it_is_unknown(
    cx: &mut TestAppContext,
) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(crate::endpoint::Endpoint::new(
                "ssh:devbox".into(),
                "Devbox".into(),
                sheprd_client::ConnectTarget::Ssh {
                    target: "me@devbox".into(),
                    session: "default".into(),
                },
                true,
            ));
            let own = view.endpoints[0].id.clone();
            let none = caller(None);

            let done = mute::set(view, &none, Some("devbox"), true, cx);
            assert!(matches!(done, Response::Done { message } if message == "Muted Devbox"));
            assert!(muted(cx, "ssh:devbox") && !muted(cx, &own));
            assert!(view.control_hosts(cx)[1].muted);

            let again = mute::set(view, &none, Some("ssh:devbox"), true, cx);
            assert!(
                matches!(again, Response::Done { .. }),
                "asking twice is fine"
            );

            assert!(matches!(
                mute::set(view, &none, None, true, cx),
                Response::Done { .. }
            ));
            assert!(muted(cx, &own));
            mute::set(view, &none, None, false, cx);
            assert!(!muted(cx, &own) && muted(cx, "ssh:devbox"));

            let unknown = mute::set(view, &none, Some("nowhere"), true, cx);
            assert_eq!(error_code(&unknown), Some(ErrorCode::NotFound));
        });
    });
}
