//! The helper's presence lines, and the lines that answer them.

use super::*;

#[test]
fn reads_presence_frames_departures_and_idle_time() {
    let line = format!(
        r#"{{"type":"presence_frame","central":"{PHONE}","data":"{}"}}"#,
        STANDARD.encode(br#"{"type":"near","near":true}"#)
    );
    assert_eq!(
        parse_line(&line),
        Some(Event::PresenceFrame {
            central: PHONE.to_owned(),
            data: br#"{"type":"near","near":true}"#.to_vec()
        })
    );
    assert_eq!(
        parse_line(&format!(r#"{{"type":"disconnected","central":"{PHONE}"}}"#)),
        Some(Event::Disconnected {
            central: PHONE.to_owned()
        })
    );
    assert_eq!(
        parse_line(r#"{"type":"idle","seconds":42}"#),
        Some(Event::Idle(42))
    );
    assert_eq!(
        parse_line(r#"{"type":"disconnected","central":"not-a-uuid"}"#),
        None
    );
}

#[test]
fn writes_presence_frames_and_turns_presence_on_and_off() {
    let value: serde_json::Value = serde_json::from_str(&presence_frame(PHONE, b"{}")).unwrap();
    assert_eq!(value["type"], "presence_frame");
    assert_eq!(value["central"], PHONE);
    let on: serde_json::Value = serde_json::from_str(&advertise_presence(true)).unwrap();
    assert_eq!(
        on,
        serde_json::json!({"type": "advertise_presence", "on": true})
    );
}
