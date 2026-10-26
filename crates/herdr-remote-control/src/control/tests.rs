use super::*;
use serde_json::json;

#[test]
fn requests_and_responses_keep_their_wire_names() {
    assert_eq!(
        serde_json::to_value(Request::Approve {
            id: PairingId("p1".into()),
            allow: true,
            code: "428913".into(),
        })
        .unwrap(),
        json!({"request": "approve", "id": "p1", "allow": true, "code": "428913"})
    );
    assert_eq!(
        serde_json::to_value(Response::Pairing(PairingState::Claimed {
            device_name: "iPhone".into(),
            code: "428913".into(),
            app: None,
        }))
        .unwrap(),
        json!({"response": "pairing", "state": "claimed", "device_name": "iPhone", "code": "428913"})
    );
    // A claim from another computer's app names it; an older daemon's
    // answer, without the field, reads as a claim from this computer.
    let from_app: Response = serde_json::from_value(json!({
        "response": "pairing", "state": "claimed", "device_name": "iPhone", "code": "428913",
        "app": "https://100.64.0.9:7443",
    }))
    .unwrap();
    assert!(matches!(
        from_app,
        Response::Pairing(PairingState::Claimed { app: Some(app), .. }) if app == "https://100.64.0.9:7443"
    ));
    let older: Response = serde_json::from_value(
        json!({"response": "pairing", "state": "claimed", "device_name": "iPhone", "code": "428913"}),
    )
    .unwrap();
    assert!(matches!(
        older,
        Response::Pairing(PairingState::Claimed { app: None, .. })
    ));
}

#[test]
fn an_unknown_error_code_from_a_newer_daemon_reads_as_other() {
    let response: Response =
        serde_json::from_value(json!({"response": "error", "code": "brand_new", "message": "x"}))
            .unwrap();
    assert_eq!(
        response,
        Response::Error {
            code: ErrorCode::Other,
            message: "x".into()
        }
    );
}

#[cfg(unix)]
#[test]
fn a_request_reaches_a_listener_and_returns_its_answer() {
    use herdr_client::private_endpoint::Listener;
    let dir = tempfile::tempdir().unwrap();
    let path = address(dir.path());
    let listener = Listener::bind(&path).unwrap();
    let server = std::thread::spawn(move || {
        let (reader, writer, _closer) = listener.accept().unwrap().split().unwrap();
        let request: Request = read_frame(&mut BufReader::new(reader)).unwrap().unwrap();
        assert_eq!(request, Request::Devices);
        let mut writer = BufWriter::new(writer);
        write_frame(
            &mut writer,
            &Response::Error {
                code: ErrorCode::UnknownDevice,
                message: "gone".into(),
            },
        )
        .unwrap();
        writer.flush().unwrap();
    });
    let error = request(&path, &Request::Devices).unwrap_err();
    server.join().unwrap();
    assert!(matches!(
        error,
        Error::Daemon {
            code: ErrorCode::UnknownDevice,
            ..
        }
    ));
}

#[test]
fn a_missing_daemon_is_a_connect_error() {
    let dir = tempfile::tempdir().unwrap();
    let error = request(&address(dir.path()), &Request::Status).unwrap_err();
    assert!(matches!(error, Error::Connect(_)));
}
