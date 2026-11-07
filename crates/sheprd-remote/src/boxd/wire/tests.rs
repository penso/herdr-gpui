#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn messages_leave_out_empty_fields_and_carry_map_entries() {
    assert!(
        Message::default()
            .string(1, "")
            .bool(2, false)
            .into_bytes()
            .is_empty()
    );
    let bytes = Message::default()
        .string(1, "sheprd-calm-otter")
        .entry(14, "sheprd", "device")
        .into_bytes();
    let fields = fields(&bytes).unwrap();
    assert_eq!(fields[0], (1, Field::Bytes(b"sheprd-calm-otter")));
    let Field::Bytes(entry) = fields[1].1 else {
        panic!("a map entry is a message");
    };
    assert_eq!(fields[1].0, 14);
    assert_eq!(
        super::fields(entry).unwrap(),
        [
            (1, Field::Bytes(&b"sheprd"[..])),
            (2, Field::Bytes(&b"device"[..]))
        ]
    );
}

#[test]
fn a_machine_reads_its_fields_and_skips_the_rest() {
    let mut bytes = Message::default()
        .string(1, "4e50-id")
        .string(2, "sheprd-calm-otter")
        .string(5, "running")
        .bool(26, true)
        .into_bytes();
    // A fixed64 and a fixed32 field a newer server might add.
    bytes.extend_from_slice(&[0x99, 0x03, 1, 2, 3, 4, 5, 6, 7, 8, 0x9d, 0x03, 1, 2, 3, 4]);
    assert_eq!(
        Machine::decode(&bytes).unwrap(),
        Machine {
            id: "4e50-id".into(),
            name: "sheprd-calm-otter".into(),
            status: "running".into(),
        }
    );
    let created = Message::default()
        .string(1, "id")
        .string(2, "n")
        .string(6, "starting")
        .into_bytes();
    assert_eq!(
        Machine::decode_created(&created).unwrap().status,
        "starting"
    );
    let list = Message::default()
        .bytes(1, &bytes)
        .bytes(1, &Message::default().string(1, "id-2").into_bytes())
        .string(2, "ssh-ed25519 AAAA")
        .into_bytes();
    let machines = Machine::decode_list(&list).unwrap();
    assert_eq!(machines.len(), 2);
    assert_eq!(machines[1].id, "id-2");
}

#[test]
fn exec_output_keeps_bytes_streams_and_negative_exit_codes() {
    let mut bytes = Message::default()
        .bytes(1, b"\x00\xff")
        .bool(10, true)
        .into_bytes();
    // exit_code = -1, an int32 as ten bytes.
    bytes.extend_from_slice(&[
        0x30, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01,
    ]);
    assert_eq!(
        Output::decode(&bytes).unwrap(),
        Output {
            data: vec![0, 255],
            stderr: true,
            exit_code: Some(-1),
        }
    );
    assert_eq!(Output::decode(&[]).unwrap(), Output::default());
}

#[test]
fn the_exec_requests_name_the_machine_command_and_stdin() {
    assert_eq!(
        fields(&exec_start("m-1", "cat")).unwrap(),
        [
            (4, Field::Bytes(&b"cat"[..])),
            (5, Field::Bytes(&b"m-1"[..]))
        ]
    );
    assert_eq!(
        fields(&exec_stdin(b"x")).unwrap(),
        [(1, Field::Bytes(&b"x"[..])), (2, Field::Varint(1))]
    );
}

#[test]
fn malformed_messages_are_refused_not_guessed() {
    for bad in [
        &[0x0a, 0x05, b'a'][..], // length past the end
        &[0x08][..],             // varint missing
        &[0x0b][..],             // wire type 3, a group
        &[
            0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01,
        ][..],
        &[0x09, 1, 2][..], // fixed64 cut short
    ] {
        assert!(matches!(fields(bad), Err(Error::Wire)), "{bad:?}");
    }
    assert!(
        Machine::decode(&[0x0a, 0x01, 0xff]).is_err(),
        "names are UTF-8"
    );
}

#[test]
fn frames_wait_for_their_whole_payload_and_are_bounded() {
    let framed = frame(b"abc");
    assert_eq!(framed, [0, 0, 0, 0, 3, b'a', b'b', b'c']);
    assert_eq!(take_frame(&framed[..4]).unwrap(), None);
    assert_eq!(take_frame(&framed[..7]).unwrap(), None);
    assert_eq!(take_frame(&framed).unwrap(), Some((0, &b"abc"[..], 8)));
    let huge = [0, 0x7f, 0xff, 0xff, 0xff];
    assert!(matches!(take_frame(&huge), Err(Error::Size)));
}

#[test]
fn trailers_read_the_status_and_decode_the_message() {
    let trailers =
        Trailers::parse(b"grpc-status: 5\r\ngrpc-message: machine%20gone\r\nx-other: 1\r\n");
    assert_eq!(
        trailers,
        Trailers {
            status: Some(5),
            message: Some("machine gone".into()),
        }
    );
    assert_eq!(Trailers::parse(b"").status, None);
}
