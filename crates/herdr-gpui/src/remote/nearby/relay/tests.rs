use super::*;
use herdr_remote_control::control::{ErrorCode, NearbyRevealed, NearbyStarted};
use std::io::Cursor;

mod presence;

const PHONE: &str = "0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9";

#[test]
fn reads_the_helpers_lines() {
    assert_eq!(
        parse_line(r#"{"type":"state","state":"powered_on"}"#),
        Some(Event::Radio(Radio::On))
    );
    assert_eq!(
        parse_line(r#"{"type":"state","state":"unauthorized"}"#),
        Some(Event::Radio(Radio::Unauthorized))
    );
    assert_eq!(
        parse_line(r#"{"type":"state","state":"later"}"#),
        Some(Event::Radio(Radio::Unknown))
    );
    let line = format!(
        r#"{{"type":"frame","central":"{PHONE}","data":"{}"}}"#,
        STANDARD.encode(b"{}")
    );
    assert_eq!(
        parse_line(&line),
        Some(Event::Frame {
            central: PHONE.to_owned(),
            data: b"{}".to_vec()
        })
    );
    assert_eq!(
        parse_line(r#"{"type":"error","message":"advertising: off"}"#),
        Some(Event::Failure("advertising: off".to_owned()))
    );
}

#[test]
fn ignores_malformed_and_oversized_lines() {
    assert_eq!(parse_line("not json"), None);
    assert_eq!(parse_line(r#"{"type":"what"}"#), None);
    assert_eq!(
        parse_line(r#"{"type":"frame","central":"x","data":"e30="}"#),
        None
    );
    assert_eq!(
        parse_line(&format!(
            r#"{{"type":"frame","central":"{PHONE}","data":"%%%"}}"#
        )),
        None
    );
    let big = STANDARD.encode(vec![b'a'; MAX_MESSAGE + 1]);
    assert_eq!(
        parse_line(&format!(
            r#"{{"type":"frame","central":"{PHONE}","data":"{big}"}}"#
        )),
        None
    );
}

#[test]
fn maps_a_phones_frame_to_its_request() {
    assert!(matches!(
        request(br#"{"type":"start","public_key":"BAAA"}"#),
        Ok(Request::NearbyStart { public_key }) if public_key == "BAAA"
    ));
    assert!(matches!(
        request(br#"{"type":"reveal","session":"s1","nonce":"n1"}"#),
        Ok(Request::NearbyReveal { session, nonce }) if session == "s1" && nonce == "n1"
    ));
    assert!(matches!(
        request(br#"{"type":"pair"}"#),
        Err(Error::Malformed(_))
    ));
    assert!(matches!(
        request(&vec![b' '; MAX_MESSAGE + 1]),
        Err(Error::TooLarge { limit: MAX_MESSAGE })
    ));
}

#[test]
fn answers_with_the_daemons_json_or_an_error() {
    let started = NearbyStarted {
        session: "s1".into(),
        computer: "m4max".into(),
        fingerprint: "AB".repeat(32),
        public_key: "PK".into(),
        commit: "C".into(),
    };
    let value: serde_json::Value =
        serde_json::from_slice(&answer(Ok(Response::NearbyStarted(started)))).unwrap();
    assert_eq!(value["session"], "s1");
    assert_eq!(value["public_key"], "PK");
    assert_eq!(value["commit"], "C");
    let revealed = NearbyRevealed {
        nonce: "N".into(),
        sealed: "S".into(),
    };
    let value: serde_json::Value =
        serde_json::from_slice(&answer(Ok(Response::NearbyRevealed(revealed)))).unwrap();
    assert_eq!(value, serde_json::json!({ "nonce": "N", "sealed": "S" }));
    let refused = Response::Error {
        code: ErrorCode::NoPairing,
        message: "open the dialog".into(),
    };
    let value: serde_json::Value = serde_json::from_slice(&answer(Ok(refused))).unwrap();
    assert_eq!(value, serde_json::json!({ "error": "open the dialog" }));
    let value: serde_json::Value = serde_json::from_slice(&answer(Ok(Response::Done))).unwrap();
    assert!(value["error"].is_string());
    let value: serde_json::Value = serde_json::from_slice(&refusal()).unwrap();
    assert!(value["error"].is_string());
}

#[test]
fn advertises_the_name_fingerprint_prefix_and_port() {
    let fingerprint = "c4:14:5a:b3:7d:31:9e:af:a3:56:b6:0e:d8:f3:e4:6b";
    let value: serde_json::Value =
        serde_json::from_str(&advertise("m4max", fingerprint, Some(7443))).unwrap();
    assert_eq!(
        value,
        serde_json::json!({ "type": "advertise", "name": "m4max", "fp": "C4145AB37D319EAF", "port": "7443" })
    );
    let value: serde_json::Value =
        serde_json::from_str(&advertise("m4max", fingerprint, None)).unwrap();
    assert!(value.get("port").is_none());
}

#[test]
fn reads_the_port_from_the_offer_link() {
    assert_eq!(port("https://192.168.1.20:7443/pair#t=a&fp=b"), Some(7443));
    assert_eq!(port("https://[fd7a:115c::1]:7544/pair#t=a"), Some(7544));
    assert_eq!(port("https://m4max.local:7443/pair"), Some(7443));
    assert_eq!(port("https://m4max.local/pair"), None);
    assert_eq!(port("https://fd7a:115c::1/pair"), None);
    assert_eq!(port("http://host:1/pair"), None);
}

#[test]
fn writes_a_frame_for_the_helper() {
    let value: serde_json::Value = serde_json::from_str(&frame(PHONE, b"{}")).unwrap();
    assert_eq!(value["type"], "frame");
    assert_eq!(value["central"], PHONE);
    assert_eq!(
        STANDARD.decode(value["data"].as_str().unwrap()).unwrap(),
        b"{}"
    );
}

#[test]
fn reads_bounded_lines_and_skips_an_overlong_one() {
    let long = "x".repeat(MAX_LINE + 10);
    let input = format!("first\n{long}\nlast");
    let mut reader = Cursor::new(input.into_bytes());
    let mut line = Vec::new();
    assert!(read_line(&mut reader, &mut line).unwrap());
    assert_eq!(line, b"first");
    assert!(read_line(&mut reader, &mut line).unwrap());
    assert!(line.is_empty());
    assert!(read_line(&mut reader, &mut line).unwrap());
    assert_eq!(line, b"last");
    assert!(!read_line(&mut reader, &mut line).unwrap());
}

#[test]
fn looks_for_the_helper_only_beside_the_executable() {
    let dir = std::env::temp_dir().join(format!("herdr-nearby-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("herdr-gpui");
    assert_eq!(helper_path(Some(&exe)), None);
    let helper = dir.join(PROGRAM);
    std::fs::write(&helper, b"#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(helper_path(Some(&exe)), Some(helper.clone()));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A stand-in helper that echoes nothing: starting and dropping it must
/// not hang, and the reaper waits for exactly that child.
#[cfg(unix)]
#[test]
fn starts_and_stops_a_helper() {
    let helper = Helper::spawn(Path::new("/bin/cat")).unwrap();
    helper.send(advertise("m4max", "AB", Some(7443)));
    drop(helper);
}
