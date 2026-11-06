#![allow(clippy::unwrap_used)]
use super::*;
use crate::config::BoxdConfig;

mod live;
mod mock;
mod relay;

/// Settings for `url` with no labels; nothing is contacted.
pub fn settings(url: &str) -> Settings {
    Settings::resolve(
        &BoxdConfig {
            api_url: Some(url.into()),
            ..BoxdConfig::default()
        },
        |_| None,
    )
    .unwrap()
    .unwrap()
}

#[test]
fn the_api_must_be_an_https_origin_or_loopback() {
    assert_eq!(
        settings("https://boxd.sh:9443/").base,
        "https://boxd.sh:9443"
    );
    assert_eq!(settings("http://127.0.0.1:9/").base, "http://127.0.0.1:9");
    for bad in [
        "http://boxd.sh:9443",
        "https://user:pass@boxd.sh:9443",
        "https://boxd.sh:9443/api",
        "https://boxd.sh:9443/?x=1",
        "ftp://boxd.sh",
        "not a url",
    ] {
        let config = BoxdConfig {
            api_url: Some(bad.into()),
            ..BoxdConfig::default()
        };
        assert!(
            matches!(Settings::resolve(&config, |_| None), Err(Error::Url(_))),
            "{bad}"
        );
    }
    assert!(
        Settings::resolve(&BoxdConfig::default(), |_| None)
            .unwrap()
            .is_none(),
        "no table, no account"
    );
}

#[test]
fn the_environment_replaces_the_api_url() {
    let resolved = Settings::resolve(&BoxdConfig::default(), |name| {
        (name == "HERDR_BOXD_API_URL").then(|| "https://nyc.boxd.sh:9443".into())
    })
    .unwrap()
    .unwrap();
    assert_eq!(resolved.base, "https://nyc.boxd.sh:9443");
}

#[test]
fn the_exec_stream_connects_to_the_api_origin() {
    let remote = settings("https://fra.boxd.sh:9443");
    assert_eq!(
        remote.endpoint().unwrap(),
        settings::Endpoint {
            host: "fra.boxd.sh".into(),
            port: 9443,
            encrypted: true,
        }
    );
    assert_eq!(remote.authority(), "fra.boxd.sh:9443");
    let default_port = settings("https://boxd.example.com");
    assert_eq!(default_port.endpoint().unwrap().port, 443);
    let local = settings("http://[::1]:7").endpoint().unwrap();
    assert_eq!((local.host.as_str(), local.encrypted), ("::1", false));
}

#[test]
fn labels_follow_boxd_rules_and_leave_the_device_label_alone() {
    let with = |key: &str, value: &str| BoxdConfig {
        api_url: Some(DEFAULT_API_URL.into()),
        labels: [(key.to_owned(), value.to_owned())].into(),
        ..BoxdConfig::default()
    };
    let good = Settings::resolve(&with("team.web/owner", "Ann"), |_| None)
        .unwrap()
        .unwrap();
    assert_eq!(good.labels["team.web/owner"], "Ann");
    for (key, value) in [
        ("Team", "x"),
        ("-team", "x"),
        ("boxd/owner", "x"),
        ("sheprd", "other"),
        ("team", "line\nbreak"),
        (&"k".repeat(64), "x"),
    ] {
        assert!(
            Settings::resolve(&with(key, value), |_| None).is_err(),
            "{key}={value}"
        );
    }
}

#[test]
fn machine_names_and_references_stay_plain() {
    use settings::{machine_name, machine_reference};
    assert!(machine_name("sheprd-calm-otter-1a"));
    for bad in [
        "",
        "1abc",
        "-abc",
        "abc-",
        "Upper",
        "a_b",
        "a b",
        &"x".repeat(64),
    ] {
        assert!(!machine_name(bad), "{bad}");
    }
    assert!(machine_reference("4e5084d5-7dca-4be4-aaa3-84a5fd11d63d"));
    for bad in ["", "a/b", "a b", "../x", "a\n"] {
        assert!(!machine_reference(bad), "{bad}");
    }
}

#[test]
fn statuses_map_to_what_a_connection_must_do() {
    use api::Readiness;
    for (status, readiness) in [
        ("running", Readiness::Ready),
        ("stopped", Readiness::Stopped),
        ("suspended", Readiness::Suspended),
        ("hibernated", Readiness::Hibernated),
        ("failed", Readiness::Failed),
        ("starting", Readiness::Busy),
        ("migrating", Readiness::Busy),
        ("something-new", Readiness::Busy),
    ] {
        assert_eq!(Readiness::from(status), readiness, "{status}");
    }
}

#[test]
fn status_messages_stay_one_short_line() {
    assert_eq!(
        http::bounded("machine\nnot found").as_deref(),
        Some("machine not found")
    );
    assert_eq!(http::bounded(" \n "), None);
    assert_eq!(http::bounded(&"x".repeat(500)).unwrap().len(), 300);
    let failure = |status| {
        http::failure(
            "test",
            &wire::Trailers {
                status: Some(status),
                message: Some("why".into()),
            },
        )
    };
    assert!(matches!(failure(16), Error::Authentication));
    assert!(matches!(failure(7), Error::Forbidden));
    let Error::Status(status) = failure(9) else {
        panic!("other codes keep their status");
    };
    assert_eq!(status.code, Code::Grpc(9));
    assert_eq!(
        status.to_string(),
        "boxd request failed (gRPC status 9): why"
    );
}

#[test]
fn the_relay_helper_needs_an_account_a_machine_and_a_command() {
    use std::ffi::OsString;
    let args = |parts: &[&str]| parts.iter().map(OsString::from).collect::<Vec<_>>();
    assert!(super::relay::run_helper(&args(&["browser"])).is_none());
    assert_eq!(
        super::relay::run_helper(&args(&[super::relay::HELPER, DEFAULT_API_URL, "m"])),
        Some(std::process::ExitCode::from(2))
    );
}
