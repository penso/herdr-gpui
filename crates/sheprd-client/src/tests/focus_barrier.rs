use super::*;

// A Windows worker has no wake to wait on: it still polls every 10 ms.
#[cfg(unix)]
mod image_wake;

fn connected() -> (Client, Stream, thread::JoinHandle<Result<()>>) {
    connected_reporting("0.9.5")
}

fn connected_reporting(version: &str) -> (Client, Stream, thread::JoinHandle<Result<()>>) {
    let (client, mut server, worker) = test_client();
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let ClientMessage::EndpointControl { .. } = receive(&mut server) else {
        panic!("hello")
    };
    let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
    welcome["server_version"] = json!(version);
    welcome["methods"] = json!([
        "client_shell.surface.set",
        "pane.focus",
        "tab.focus",
        "workspace.focus",
        "worktree.create"
    ]);
    welcome["capabilities"] = json!(["surface_interest", "presentation_effects_fence"]);
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_WELCOME_KIND.into(),
            data: welcome.to_string(),
        },
    );
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: SNAPSHOT.into(),
        },
    );
    assert!(matches!(event(&client), ClientEvent::Connected(_)));
    assert!(matches!(event(&client), ClientEvent::Snapshot(_)));
    (client, server, worker)
}

fn request(server: &mut Stream) -> Value {
    let ClientMessage::ClientShellEndpointRequest { request, .. } = receive(server) else {
        panic!("expected an API request")
    };
    serde_json::from_str(&request).unwrap()
}

fn respond(server: &mut Stream, boot_id: &str, id: &str, body: Value) {
    let mut response = json!({"id": id});
    response
        .as_object_mut()
        .unwrap()
        .extend(body.as_object().unwrap().clone());
    send(
        server,
        ServerMessage::ClientShellEndpointResponseChunk {
            boot_id: boot_id.into(),
            request_id: id.into(),
            final_chunk: true,
            data: response.to_string().into_bytes(),
        },
    );
}

fn activated(revision: u64) -> Value {
    json!({"result": {"type": "client_shell_surface_set", "active": true, "projection_revision": revision}})
}

fn response(client: &Client) -> (String, Value) {
    match event(client) {
        ClientEvent::Response {
            request_id,
            response,
        } => (request_id, response),
        other => panic!("expected a response, got {other:?}"),
    }
}

fn silent(server: &mut Stream) {
    server
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    assert!(matches!(
        server.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    ));
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
}

#[test]
fn activation_follows_focus_unacknowledged_and_responses_keep_send_order() {
    for method in ["pane.focus", "tab.focus", "workspace.focus"] {
        let (client, mut server, worker) = connected();
        let focus = match method {
            "pane.focus" => client.handle.focus_pane("boot-v1", "w1:p2"),
            "tab.focus" => client.handle.focus_tab("boot-v1", "w1:t2"),
            _ => client.handle.focus_workspace("boot-v1", "w2"),
        }
        .unwrap();
        let activation = client.handle.set_surface_active("boot-v1", true).unwrap();
        client.handle.set_focus("boot-v1", false).unwrap();
        let next = client.handle.focus_pane("boot-v1", "w1:p3").unwrap();
        assert_eq!(request(&mut server)["method"], method);
        // Both requests are on the wire before either response.
        let barrier = request(&mut server);
        assert_eq!(barrier["id"], activation);
        assert_eq!(barrier["params"], json!({"active": true}));
        assert_eq!(
            receive(&mut server),
            ClientMessage::ClientShellFocus { focused: false }
        );
        // Any other request still waits for the whole transaction.
        silent(&mut server);
        // Herdr answers the activation before the focus response bridge.
        respond(&mut server, "boot-v1", &activation, activated(9));
        silent(&mut server);
        assert!(client.events.try_recv().is_err());
        respond(&mut server, "boot-v1", &focus, json!({"result": {}}));
        assert_eq!(response(&client).0, focus);
        let (id, ack) = response(&client);
        assert_eq!(id, activation);
        assert_eq!(ack["result"]["projection_revision"], 9);
        assert_eq!(request(&mut server)["id"], next);
        client.handle.disconnect();
        worker.join().unwrap().unwrap();
    }
}

#[test]
fn only_checked_stable_releases_pipeline_activation() {
    for (version, pipelines) in [
        ("0.9.0", true),
        ("0.9.5", true),
        ("0.8.2", false),
        ("0.9.6", false),
        ("0.9.5-preview.1a2b", false),
        ("1.0.0", false),
        ("0.9", false),
        // Only the exact release strings that were checked.
        ("00.9.5", false),
        ("0.09.5", false),
        ("0.9.05", false),
        ("0.+9.5", false),
        ("+0.9.5", false),
        ("v0.9.3", false),
        (" 0.9.3", false),
        ("0.9.3.0", false),
    ] {
        let (client, mut server, worker) = connected_reporting(version);
        let focus = client.handle.focus_pane("boot-v1", "w1:p2").unwrap();
        let activation = client.handle.set_surface_active("boot-v1", true).unwrap();
        assert_eq!(request(&mut server)["id"], focus);
        if pipelines {
            assert_eq!(request(&mut server)["id"], activation, "{version}");
        } else {
            silent(&mut server);
            respond(&mut server, "boot-v1", &focus, json!({"result": {}}));
            assert_eq!(response(&client).0, focus);
            assert_eq!(request(&mut server)["id"], activation, "{version}");
        }
        client.handle.disconnect();
        worker.join().unwrap().unwrap();
    }
}

#[test]
fn failed_focus_is_delivered_before_its_activation() {
    let (client, mut server, worker) = connected();
    let focus = client.handle.focus_pane("boot-v1", "missing").unwrap();
    let activation = client.handle.set_surface_active("boot-v1", true).unwrap();
    request(&mut server);
    request(&mut server);
    respond(&mut server, "boot-v1", &activation, activated(9));
    respond(
        &mut server,
        "boot-v1",
        &focus,
        json!({"error": {"code": "pane_not_found", "message": "pane not found"}}),
    );
    let (id, failed) = response(&client);
    assert_eq!(id, focus);
    assert_eq!(failed["error"]["code"], "pane_not_found");
    assert_eq!(response(&client).0, activation);
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn other_requests_keep_activation_behind_their_response() {
    let (client, mut server, worker) = connected();
    // Deferred worktree navigation is fenced by the revision activation advances.
    let create = client
        .handle
        .request(
            "boot-v1",
            Method::WorktreeCreate,
            json!({"name": "public", "focus": true}),
        )
        .unwrap();
    let activation = client.handle.set_surface_active("boot-v1", true).unwrap();
    assert_eq!(request(&mut server)["id"], create);
    silent(&mut server);
    respond(&mut server, "boot-v1", &create, json!({"result": {}}));
    assert_eq!(response(&client).0, create);
    assert_eq!(request(&mut server)["id"], activation);
    respond(&mut server, "boot-v1", &activation, activated(9));
    assert_eq!(response(&client).0, activation);

    // One activation at a time: a second waits behind the first transaction.
    let focus = client.handle.focus_pane("boot-v1", "w1:p2").unwrap();
    let first = client.handle.set_surface_active("boot-v1", true).unwrap();
    let second = client.handle.set_surface_active("boot-v1", true).unwrap();
    request(&mut server);
    assert_eq!(request(&mut server)["id"], first);
    silent(&mut server);
    respond(&mut server, "boot-v1", &first, activated(10));
    respond(&mut server, "boot-v1", &focus, json!({"result": {}}));
    assert_eq!(response(&client).0, focus);
    assert_eq!(response(&client).0, first);
    assert_eq!(request(&mut server)["id"], second);
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn refused_activation_is_resent_once_after_focus_and_stops_pipelining() {
    let (client, mut server, worker) = connected();
    let focus = client.handle.focus_pane("boot-v1", "w1:p2").unwrap();
    let activation = client.handle.set_surface_active("boot-v1", true).unwrap();
    request(&mut server);
    let original = request(&mut server);
    respond(
        &mut server,
        "boot-v1",
        &activation,
        json!({"error": {"code": "endpoint_busy", "message": "busy"}}),
    );
    silent(&mut server);
    respond(&mut server, "boot-v1", &focus, json!({"result": {}}));
    assert_eq!(response(&client).0, focus);
    // The refusal is not reported; focus is never sent again.
    assert_eq!(request(&mut server), original);
    respond(&mut server, "boot-v1", &activation, activated(9));
    assert_eq!(response(&client).0, activation);

    let focus = client.handle.focus_pane("boot-v1", "w1:p3").unwrap();
    let activation = client.handle.set_surface_active("boot-v1", true).unwrap();
    assert_eq!(request(&mut server)["id"], focus);
    silent(&mut server);
    respond(&mut server, "boot-v1", &focus, json!({"result": {}}));
    assert_eq!(response(&client).0, focus);
    assert_eq!(request(&mut server)["id"], activation);
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn reboot_and_cancellation_drop_a_refused_activation() {
    for reboot in [true, false] {
        let (client, mut server, worker) = connected();
        client.handle.focus_pane("boot-v1", "w1:p2").unwrap();
        let activation = client.handle.set_surface_active("boot-v1", true).unwrap();
        request(&mut server);
        request(&mut server);
        respond(
            &mut server,
            "boot-v1",
            &activation,
            json!({"error": {"code": "endpoint_busy", "message": "busy"}}),
        );
        if reboot {
            // A boot never changes within a connection, so the waiting
            // activation ends with it and is never written to the new daemon.
            let mut rebooted: Value = serde_json::from_str(SNAPSHOT).unwrap();
            rebooted["boot_id"] = json!("boot-v2");
            send(
                &mut server,
                ServerMessage::EndpointControl {
                    kind: ENDPOINT_SNAPSHOT_KIND.into(),
                    data: rebooted.to_string(),
                },
            );
            assert!(matches!(
                worker.join().unwrap(),
                Err(Error::SnapshotIdentity)
            ));
        } else {
            client.handle.disconnect();
            worker.join().unwrap().unwrap();
        }
        assert_eq!(server.read(&mut [0]).unwrap(), 0);
        assert!(client.events.try_recv().is_err());
    }
}

#[test]
fn a_waiting_activation_shares_the_request_deadline() {
    let mut session = ready_session();
    let stale = Instant::now() - COMMAND_TIMEOUT - Duration::from_millis(1);
    session.barrier = Some(session::Barrier {
        pending: Pending {
            id: "activation".into(),
            method: Method::ClientShellSurfaceSet,
            bytes: Vec::new(),
            started: stale,
        },
        request: Vec::new(),
        held: None,
        refused: false,
    });
    assert_eq!(session.deadline(), Some(stale + COMMAND_TIMEOUT));
    assert!(matches!(
        session.check_timeouts(),
        Err(Error::RequestTimeout)
    ));
}

#[test]
fn a_held_activation_response_counts_toward_the_shared_response_bound() {
    let chunk = |id: &str, data: Vec<u8>| ServerMessage::ClientShellEndpointResponseChunk {
        boot_id: "boot-v1".into(),
        request_id: id.into(),
        final_chunk: true,
        data,
    };
    let pending = |id: &str, method| Pending {
        id: id.into(),
        method,
        bytes: Vec::new(),
        started: Instant::now(),
    };
    for excess in [0, 1] {
        let mut session = ready_session();
        session.pending = Some(pending("focus", Method::PaneFocus));
        session.barrier = Some(session::Barrier {
            pending: pending("activation", Method::ClientShellSurfaceSet),
            request: Vec::new(),
            held: None,
            refused: false,
        });
        let mut held = br#"{"id":"activation","result":{}}"#.to_vec();
        held.resize(MAX_RESPONSE_BYTES / 2, b' ');
        let held_len = held.len();
        session
            .handle_message(chunk("activation", held), |_| -> Result<()> {
                panic!("held until focus completes")
            })
            .unwrap();
        let mut focus = br#"{"id":"focus","result":{}}"#.to_vec();
        focus.resize(MAX_RESPONSE_BYTES - held_len + excess, b' ');
        let mut events = Vec::new();
        let result = session.handle_message(chunk("focus", focus), |event| {
            events.push(event);
            Ok(())
        });
        if excess == 0 {
            result.unwrap();
            let ids: Vec<_> = events
                .iter()
                .map(|event| match event {
                    ClientEvent::Response { request_id, .. } => request_id.as_str(),
                    other => panic!("unexpected {other:?}"),
                })
                .collect();
            assert_eq!(ids, ["focus", "activation"]);
            assert!(session.barrier.is_none());
        } else {
            assert!(matches!(result, Err(Error::ResponseLimit)));
            assert!(events.is_empty());
        }
    }
}
