use super::*;

#[gpui::test]
fn a_host_is_selected_by_id_or_label_and_a_refusal_says_why(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut off = crate::endpoint::Endpoint::new(
                "ssh:devbox".into(),
                "Devbox".into(),
                herdr_client::ConnectTarget::Ssh {
                    target: "me@devbox".into(),
                    session: "default".into(),
                },
                false,
            );
            off.enabled = false;
            view.endpoints.push(off);
            let mut chosen = Vec::new();
            let shown = select(view, "LOCAL", |_, id| {
                chosen.push(id.to_owned());
                true
            });
            assert_eq!(shown, Response::done("Showing Local"));
            assert_eq!(chosen, ["local"]);
            let missing = select(view, "nowhere", |_, _| true);
            assert_eq!(error_code(&missing), Some(ErrorCode::NotFound));
            let disabled = select(view, "ssh:devbox", |_, _| true);
            assert_eq!(error_code(&disabled), Some(ErrorCode::InvalidRequest));
            // The window's own reason for refusing is passed on.
            let failed = select(view, "local", |view, _| {
                view.local_error =
                    Some("Current endpoint does not support surface switching".into());
                false
            });
            assert_eq!(
                failed,
                Response::error(
                    ErrorCode::Failed,
                    "Current endpoint does not support surface switching"
                )
            );
        })
    });
}

#[gpui::test]
fn usage_and_load_say_why_they_are_empty(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let Response::Host {
                reply:
                    Reply::Usage {
                        host,
                        usage: readings,
                        note: Some(note),
                    },
            } = usage(view, SystemTime::now())
            else {
                panic!("expected usage");
            };
            assert_eq!(host, "Local");
            assert!(readings.is_empty());
            assert!(note.contains("not read usage"), "{note}");
            view.config.usage.show = false;
            let Response::Host {
                reply: Reply::Usage {
                    note: Some(note), ..
                },
            } = usage(view, SystemTime::now())
            else {
                panic!("expected usage");
            };
            assert!(note.contains("turned off"), "{note}");
            let Response::Host {
                reply:
                    Reply::System {
                        load: None,
                        note: Some(note),
                    },
            } = system(view)
            else {
                panic!("expected no load");
            };
            assert!(note.contains("samples load only while"), "{note}");
        })
    });
}
