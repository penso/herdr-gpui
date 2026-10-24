#![allow(clippy::unwrap_used)]
use super::*;
use herdr_remote_control::control::DeviceId;

fn device(id: &str, name: &str) -> Device {
    Device {
        id: DeviceId(id.into()),
        name: name.into(),
        created_ms: 0,
        last_seen_ms: None,
        push: false,
        connected: false,
    }
}

fn code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error { code, .. } => Some(*code),
        _ => None,
    }
}

#[test]
fn a_device_is_named_by_its_id_or_its_one_name() {
    let devices = [
        device("d1", "Pixel"),
        device("d2", "iPhone"),
        device("d3", "iPhone"),
        device("d4", "d1"),
    ];
    assert_eq!(pick(&devices, "d2").unwrap().id.0, "d2");
    assert_eq!(pick(&devices, "Pixel").unwrap().id.0, "d1");
    // An ID wins over a name that happens to match it.
    assert_eq!(pick(&devices, "d1").unwrap().id.0, "d1");
    assert_eq!(
        code(&pick(&devices, "iPhone").unwrap_err()),
        Some(ErrorCode::InvalidRequest)
    );
    assert_eq!(
        code(&pick(&devices, "pixel").unwrap_err()),
        Some(ErrorCode::NotFound),
        "names match exactly"
    );
    assert_eq!(
        code(&pick(&[], "Pixel").unwrap_err()),
        Some(ErrorCode::NotFound)
    );
}

#[test]
fn a_daemon_answer_becomes_devices_or_a_failure() {
    assert_eq!(
        devices(Ok(herdr_remote_control::control::Response::Devices {
            devices: vec![device("d1", "Pixel")]
        }))
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        code(&devices(Ok(herdr_remote_control::control::Response::Done)).unwrap_err()),
        Some(ErrorCode::Failed)
    );
    assert_eq!(
        code(&devices(Err(herdr_remote_control::control::Error::NoAnswer)).unwrap_err()),
        Some(ErrorCode::Failed)
    );
}
