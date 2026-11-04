use crate::{
    framing::chunks,
    peripheral::{Backend, Core, Shared, serve},
    protocol::{Channel, Command, Event, Out},
};
use std::sync::{Arc, Mutex, mpsc};

#[test]
fn info_names_the_computer_only_while_pairing() {
    let mut core = Core::new(1);
    assert_eq!(core.info(), None);
    core.apply_for_tests(Command::Presence(true));
    assert_eq!(core.info().unwrap(), br#"{"v":1}"#);
    core.apply_for_tests(Command::Advertise {
        name: "Studio".into(),
        fingerprint: "ABCD".into(),
    });
    assert_eq!(
        core.info().unwrap(),
        br#"{"v":1,"name":"Studio","fp":"ABCD"}"#
    );
    core.apply_for_tests(Command::Stop);
    assert_eq!(core.info().unwrap(), br#"{"v":1}"#);
}

#[test]
fn writes_are_allowed_only_for_what_is_advertised() {
    let mut core = Core::new(1);
    assert!(!core.write_allowed(Channel::Pair));
    assert!(!core.write_allowed(Channel::Presence));
    core.apply_for_tests(Command::Presence(true));
    assert!(core.write_allowed(Channel::Presence));
    assert!(!core.write_allowed(Channel::Pair));
    assert!(core.wants_advertising());
}

#[test]
fn phones_get_stable_uuid_shaped_ids_herdr_ui_accepts() {
    let mut core = Core::new(42);
    let id = core.id("/org/bluez/hci0/dev_AA_BB");
    assert_eq!(id.len(), 36);
    assert!(
        id.bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    );
    assert_eq!(core.id("/org/bluez/hci0/dev_AA_BB"), id);
    assert_ne!(core.id("/org/bluez/hci0/dev_CC_DD"), id);
    assert_ne!(Core::new(43).id("/org/bluez/hci0/dev_AA_BB"), id);
    assert_eq!(core.device(&id), Some("/org/bluez/hci0/dev_AA_BB"));
}

#[test]
fn a_whole_message_becomes_a_frame_and_leaving_presence_is_reported_once() {
    let mut core = Core::new(1);
    let message = br#"{"type":"near","near":true}"#;
    let mut event = None;
    for chunk in chunks(message, 20) {
        event = core.received("dev", Channel::Presence, &chunk);
    }
    let id = core.id("dev");
    assert_eq!(event, Some(Event::frame(&id, Channel::Presence, message)));
    assert_eq!(core.left("dev", Channel::Pair), None);
    assert_eq!(
        core.left("dev", Channel::Presence),
        Some(Event::Disconnected { central: id })
    );
    assert_eq!(core.left("dev", Channel::Presence), None);
}

/// Records what the loop asks of the radio.
#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<String>>,
}

impl Backend for Recorder {
    fn refresh(&self, advertise: bool) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("refresh {advertise}"));
    }

    fn notify(&self, device: &str, channel: Channel, chunks: Vec<Vec<u8>>) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("notify {device} {channel:?} {}", chunks.len()));
    }

    fn idle_seconds(&self) -> Option<u64> {
        None
    }
}

#[test]
fn the_loop_advertises_and_answers_the_phone_it_names() {
    let core: Shared = Arc::new(Mutex::new(Core::new(1)));
    let id = core.lock().unwrap().id("dev");
    core.lock().unwrap().set_chunk("dev", 100);
    let (commands, received) = mpsc::channel();
    commands.send(Command::Presence(true)).unwrap();
    commands
        .send(Command::Frame {
            central: id,
            channel: Channel::Presence,
            data: vec![1; 150],
        })
        .unwrap();
    commands
        .send(Command::Frame {
            central: "unknown".into(),
            channel: Channel::Pair,
            data: vec![1],
        })
        .unwrap();
    commands.send(Command::Presence(false)).unwrap();
    drop(commands);
    let recorder = Recorder::default();
    serve(&core, &recorder, &received, &Out);
    assert_eq!(
        *recorder.calls.lock().unwrap(),
        ["refresh true", "notify dev Presence 2", "refresh false"]
    );
}
