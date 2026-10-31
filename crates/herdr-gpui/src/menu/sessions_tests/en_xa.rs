//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the sessions popup
//! and its add and delete dialogs: each draws only translated messages,
//! except for session, device, and host names.
use super::*;
use crate::i18n::leak_tests::{assert_translated, under_en_xa};
use crate::menu::sessions::{Edit, Row, Target};
use herdr_client::RemoteHost;

/// The sessions, devices (and the headings that name them in capitals), and
/// hosts the fixture names. A device that has not answered shows its
/// connection status, which `crate::state` words and does not translate yet.
fn names() -> Vec<String> {
    [
        "default",
        "work",
        "agents",
        "main",
        "Build",
        "BUILD",
        "Prod",
        "PROD",
        "build.invalid",
        "prod.invalid",
    ]
    .map(String::from)
    .into_iter()
    .chain([crate::state::ConnectionStatus::Connecting.to_string()])
    .collect()
}

#[test]
fn the_sessions_popup_and_its_dialogs_show_only_messages_under_en_xa() {
    under_en_xa("sessions en-XA leak", |cx, recorder| {
        let (view, cx) = device_window(cx, Some(reported(&[("default", true), ("agents", false)])));
        view.update(cx, |view, cx| {
            view.endpoints[1].live.status = crate::state::ConnectionStatus::Connected;
            cx.notify();
        });
        draw(cx);
        recorder.take();
        click(cx, "device-sessions");
        draw(cx);
        assert_translated("Sessions", &recorder.take(), &names());

        // The device's two sessions come first, then its Add row.
        click(cx, "sessions-row-2");
        draw(cx);
        assert_translated("Sessions > Add", &recorder.take(), &names());

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.menu.session_edit = Some(Edit::Delete {
                    row: Row::Device {
                        id: "ssh:build".into(),
                        session: "agents".into(),
                    },
                    target: Target::Device {
                        id: "ssh:build".into(),
                        host: RemoteHost::Ssh("build.invalid".into()),
                    },
                });
                window.focus(&view.menu.focus, cx);
                cx.notify();
            })
        });
        draw(cx);
        assert_translated("Sessions > Delete", &recorder.take(), &names());
    });
}
