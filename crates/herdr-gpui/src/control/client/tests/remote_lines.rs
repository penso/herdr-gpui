use super::*;
use crate::control::protocol::remote::Reply;
use herdr_remote_control::control::{Device, DeviceId, Status};

fn status() -> Status {
    Status {
        version: "20261022.1".into(),
        port: 7443,
        addresses: vec![
            "https://192.168.1.20:7443".into(),
            "https://100.64.0.3:7443".into(),
        ],
        fingerprint: "5F:2A:C1:9E".into(),
        herdr: false,
        chats: 3,
        devices: 2,
        connected: 1,
    }
}

#[test]
fn status_prints_where_it_serves_and_who_is_connected() {
    let output = remote::output(&Reply::Status {
        enabled: true,
        status: Some(Box::new(status())),
        error: None,
    })
    .unwrap();
    assert_eq!(output.status, 0);
    assert_eq!(
        output.text,
        "Running on port 7443 (version 20261022.1)\n\
         Devices: 2 paired, 1 connected\n\
         Chats: 3\n\
         Herdr is not running on this computer\n\
         Certificate authority: 5F:2A:C1:9E\n\
         Address: https://192.168.1.20:7443\n\
         Address: https://100.64.0.3:7443"
    );
    assert_eq!(output.json["running"], true);
    assert_eq!(output.json["enabled"], true);
    assert_eq!(output.json["status"]["connected"], 1);
    assert_eq!(
        output.json["status"]["addresses"][1],
        "https://100.64.0.3:7443"
    );
}

#[test]
fn status_says_why_it_is_not_running() {
    let off = remote::output(&Reply::Status {
        enabled: false,
        status: None,
        error: None,
    })
    .unwrap();
    assert_eq!(off.text, "Remote control is off");
    assert_eq!(off.json["running"], false);
    assert_eq!(off.json["status"], serde_json::Value::Null);
    let down = remote::output(&Reply::Status {
        enabled: true,
        status: None,
        error: Some("could not reach the remote control daemon".into()),
    })
    .unwrap();
    assert_eq!(
        down.text,
        "Remote control is on but not running: could not reach the remote control daemon"
    );
    assert_eq!(
        down.json["error"],
        "could not reach the remote control daemon"
    );
}

#[test]
fn devices_print_one_line_each_or_exit_4_when_there_are_none() {
    let now = 10 * 24 * 3_600_000;
    let devices = vec![
        Device {
            id: DeviceId("d1".into()),
            name: "Pixel".into(),
            created_ms: 0,
            last_seen_ms: Some(now - 2 * 3_600_000),
            push: true,
            connected: false,
        },
        Device {
            id: DeviceId("d2".into()),
            name: "iPad".into(),
            created_ms: 0,
            last_seen_ms: Some(now),
            push: false,
            connected: true,
        },
        Device {
            id: DeviceId("d3".into()),
            name: "Old phone".into(),
            created_ms: 0,
            last_seen_ms: None,
            push: false,
            connected: false,
        },
    ];
    assert_eq!(
        remote::line(&devices[0], now),
        "d1  Pixel  last seen 2 h ago  notifications on"
    );
    assert_eq!(
        remote::line(&devices[1], now),
        "d2  iPad  connected  notifications off"
    );
    assert_eq!(
        remote::line(&devices[2], now),
        "d3  Old phone  never connected  notifications off"
    );
    let listed = remote::output(&Reply::Devices {
        devices: devices.clone(),
    })
    .unwrap();
    assert_eq!(listed.text.lines().count(), 3);
    assert_eq!(listed.json["devices"][0]["id"], "d1");
    assert_eq!(listed.json["devices"][1]["connected"], true);
    let none = remote::output(&Reply::Devices {
        devices: Vec::new(),
    })
    .unwrap();
    assert_eq!(none.status, exit::NOTHING);
    assert_eq!(none.text, "No paired devices");
}
