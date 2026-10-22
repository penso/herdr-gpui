use super::*;

fn copy(state: CopyState) -> RemoteCopy {
    RemoteCopy {
        host: "me@devbox".into(),
        path: "/srv/notes.md".into(),
        local: "/cache/1/notes.md".into(),
        state,
        auto_upload: false,
    }
}

#[test]
fn the_remote_files_request_has_a_stable_wire_form() {
    let request = Request::Host(Call {
        caller: Caller::default(),
        op: HostOp::RemoteFiles,
    });
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        text,
        r#"{"method":"host","caller":{"pane_id":null,"workspace_id":null,"daemon_socket":null},"op":{"op":"remote_files"}}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
}

#[test]
fn every_copy_state_has_its_own_name_on_the_wire() {
    let states = [
        (CopyState::Synced, "synced"),
        (CopyState::Edited, "edited"),
        (CopyState::Asking, "asking"),
        (CopyState::Uploading, "uploading"),
        (CopyState::Conflict, "conflict"),
        (CopyState::Declined, "declined"),
        (CopyState::Refetching, "refetching"),
    ];
    for (state, name) in states {
        let json = serde_json::to_value(copy(state)).unwrap();
        assert_eq!(json["state"], name);
        assert_eq!(
            serde_json::from_value::<RemoteCopy>(json).unwrap(),
            copy(state)
        );
    }
    let unknown = serde_json::json!({
        "host": "h", "path": "p", "local": "l", "state": "lost", "auto_upload": false
    });
    assert!(serde_json::from_value::<RemoteCopy>(unknown).is_err());
}

#[test]
fn the_remote_files_reply_round_trips() {
    let reply = Reply::RemoteFiles {
        copies: vec![
            copy(CopyState::Conflict),
            RemoteCopy {
                auto_upload: true,
                ..copy(CopyState::Synced)
            },
        ],
        omitted: 2,
    };
    let response = Response::Host { reply };
    let text = serde_json::to_string(&response).unwrap();
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    // An answer from an app that counts no omitted copies still reads.
    let bare = r#"{"status":"host","reply":{"kind":"remote_files","copies":[]}}"#;
    let parsed: Response = serde_json::from_str(bare).unwrap();
    assert_eq!(
        parsed,
        Response::Host {
            reply: Reply::RemoteFiles {
                copies: Vec::new(),
                omitted: 0
            }
        }
    );
}
