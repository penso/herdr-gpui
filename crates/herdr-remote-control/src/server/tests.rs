use super::{
    options::{Certificate, Invocation, ParseError, parse},
    *,
};
use std::{ffi::OsString, net::IpAddr, path::PathBuf};

mod control_socket;
mod shutdown;

fn args(list: &[&str]) -> Vec<OsString> {
    std::iter::once("herdr-remote-control")
        .chain(list.iter().copied())
        .map(OsString::from)
        .collect()
}

fn serve(list: &[&str]) -> Options {
    match parse(args(list)).unwrap() {
        Invocation::Serve(options) => options,
        other => panic!("expected serve, got {other:?}"),
    }
}

#[test]
fn serve_takes_its_defaults() {
    let options = serve(&["serve", "--state-dir", "/tmp/remote"]);
    assert_eq!(options.state_dir, PathBuf::from("/tmp/remote"));
    assert_eq!(options.port, 7443);
    assert!(options.bind.is_unspecified());
    assert_eq!(options.certificate, Certificate::Own);
    assert_eq!(options.privacy, Privacy::Generic);
    assert_eq!(options.device_idle, Duration::from_secs(30 * 24 * 60 * 60));
}

#[test]
fn every_option_is_read() {
    let options = serve(&[
        "serve",
        "--state-dir",
        "/s",
        "--port",
        "8443",
        "--bind",
        "192.168.1.20",
        "--cert",
        "/c.pem",
        "--key",
        "/k.pem",
        "--privacy",
        "excerpt",
        "--device-idle-days",
        "7",
    ]);
    assert_eq!(options.port, 8443);
    assert_eq!(options.bind, "192.168.1.20".parse::<IpAddr>().unwrap());
    assert_eq!(
        options.certificate,
        Certificate::Files {
            cert: "/c.pem".into(),
            key: "/k.pem".into()
        }
    );
    assert_eq!(options.privacy, Privacy::Excerpt);
    assert_eq!(options.device_idle, Duration::from_secs(7 * 24 * 60 * 60));
}

#[test]
fn bad_command_lines_are_refused_before_anything_starts() {
    assert_eq!(parse(args(&["serve"])).unwrap_err(), ParseError::NoStateDir);
    assert_eq!(
        parse(args(&["serve", "--state-dir", "/s", "--cert", "/c"])).unwrap_err(),
        ParseError::CertificatePair
    );
    assert_eq!(
        parse(args(&["serve", "--state-dir", "/s", "--port", "0"])).unwrap_err(),
        ParseError::Value { option: "--port" }
    );
    assert_eq!(
        parse(args(&["serve", "--state-dir", "/s", "--bind", "nowhere"])).unwrap_err(),
        ParseError::Value { option: "--bind" }
    );
    assert_eq!(
        parse(args(&["serve", "--state-dir"])).unwrap_err(),
        ParseError::Missing("--state-dir")
    );
    assert_eq!(
        parse(args(&[
            "serve",
            "--state-dir",
            "/s",
            "--device-idle-days",
            "0"
        ]))
        .unwrap_err(),
        ParseError::Value {
            option: "--device-idle-days"
        }
    );
    assert!(matches!(
        parse(args(&["serve", "--frob"])),
        Err(ParseError::Unknown(_))
    ));
    assert_eq!(parse(args(&[])).unwrap(), Invocation::Help);
    assert_eq!(parse(args(&["--version"])).unwrap(), Invocation::Version);
}

#[test]
fn fingerprints_read_as_colon_separated_hex() {
    assert_eq!(colon_hex(&[0x5f, 0x2a, 0x01]), "5F:2A:01");
}

#[test]
fn serve_takes_the_daemon_and_chat_locations_herdr_ui_resolved() {
    let options = serve(&["serve", "--state-dir", "/s"]);
    assert_eq!((options.herdr_socket, options.chat_dir), (None, None));
    let options = serve(&[
        "serve",
        "--state-dir",
        "/s",
        "--herdr-socket",
        "/h/herdr-client.sock",
        "--chat-dir",
        "/h/acp",
    ]);
    assert_eq!(
        options.herdr_socket,
        Some(PathBuf::from("/h/herdr-client.sock"))
    );
    assert_eq!(options.chat_dir, Some(PathBuf::from("/h/acp")));
    assert_eq!(
        parse(args(&["serve", "--state-dir", "/s", "--chat-dir"])),
        Err(ParseError::Missing("--chat-dir"))
    );
}
