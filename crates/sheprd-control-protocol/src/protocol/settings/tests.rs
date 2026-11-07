#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::{Call, Caller, Request, Response};

fn request(op: SettingsOp) -> Request {
    Request::Settings(Call {
        caller: Caller::default(),
        op,
    })
}

#[test]
fn requests_round_trip_with_a_stable_form() {
    for op in [
        SettingsOp::ReloadConfig,
        SettingsOp::CheckConfig,
        SettingsOp::Theme {
            name: "light:Nord,dark:Dracula".into(),
        },
        SettingsOp::Fonts,
        SettingsOp::Font { name: None },
        SettingsOp::Font {
            name: Some("JetBrains Mono".into()),
        },
        SettingsOp::Keybinds,
        SettingsOp::Caffeine {
            switch: Some(Switch::On),
            seconds: Some(1800),
        },
        SettingsOp::Caffeine {
            switch: None,
            seconds: None,
        },
        SettingsOp::Sound {
            sound: Sound::Request,
        },
        SettingsOp::Bell,
        SettingsOp::CheckUpdate,
        SettingsOp::Update,
        SettingsOp::Integrations,
    ] {
        let request = request(op);
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    }
    assert_eq!(
        serde_json::to_string(&request(SettingsOp::Sound { sound: Sound::Done })).unwrap(),
        r#"{"method":"settings","caller":{"pane_id":null,"workspace_id":null,"daemon_socket":null},"op":{"op":"sound","sound":"done"}}"#
    );
    let minimal: Request =
        serde_json::from_str(r#"{"method":"settings","caller":{},"op":{"op":"caffeine"}}"#)
            .unwrap();
    assert_eq!(
        minimal,
        request(SettingsOp::Caffeine {
            switch: None,
            seconds: None
        })
    );
}

#[test]
fn unknown_operations_and_fields_are_rejected() {
    for invalid in [
        r#"{"method":"settings","caller":{},"op":{"op":"install_skill"}}"#,
        r#"{"method":"settings","caller":{},"op":{"op":"install_integration","id":"x"}}"#,
        r#"{"method":"settings","caller":{},"op":{"op":"font","name":"A","size":3}}"#,
        r#"{"method":"settings","caller":{},"op":{"op":"sound","sound":"chime"}}"#,
        r#"{"method":"settings","caller":{},"op":{"op":"theme"}}"#,
    ] {
        assert!(
            serde_json::from_str::<Request>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn replies_round_trip() {
    for reply in [
        Reply::Config {
            missing_fonts: vec!["Iosevka".into()],
        },
        Reply::Fonts {
            fonts: vec!["Menlo".into()],
        },
        Reply::Keybinds {
            bindings: vec![BindingInfo {
                name: "toggle_sidebar".into(),
                label: "Toggle Sidebar".into(),
                keys: vec!["cmd-b".into()],
            }],
            custom: vec![CustomInfo {
                label: "Lazygit".into(),
                keys: "ctrl-b g".into(),
                reach: "shadowed".into(),
            }],
        },
        Reply::Caffeine {
            state: "on".into(),
            seconds: Some(60),
        },
        Reply::Update {
            state: "disabled".into(),
            latest: None,
            detail: Some("Local build".into()),
        },
        Reply::Integrations {
            integrations: vec![IntegrationInfo {
                id: "claude".into(),
                label: "Claude Code".into(),
                status: "current".into(),
            }],
        },
    ] {
        let response = Response::Settings { reply };
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    }
    // Optional details are left out rather than sent as null.
    assert_eq!(
        serde_json::to_string(&Reply::Caffeine {
            state: "off".into(),
            seconds: None
        })
        .unwrap(),
        r#"{"kind":"caffeine","state":"off"}"#
    );
}

#[test]
fn slow_operations_extend_the_answer_timeout() {
    assert_eq!(request(SettingsOp::Fonts).wait_seconds(), LIST_WAIT_SECONDS);
    assert_eq!(
        request(SettingsOp::CheckUpdate).wait_seconds(),
        LIST_WAIT_SECONDS
    );
    assert_eq!(
        request(SettingsOp::Font { name: None }).wait_seconds(),
        SAVE_WAIT_SECONDS
    );
    assert_eq!(request(SettingsOp::Bell).wait_seconds(), 0);
}

mod language;
