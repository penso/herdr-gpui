use std::io::{self, Cursor as Bytes, Read};

use super::*;

#[test]
fn frames_round_trip() {
    let message = ServerMessage::Change {
        change: Change::Push {
            entry: agent(1, "hello"),
        },
        cursor: Cursor {
            epoch: uuid::Uuid::nil(),
            seq: 3,
        },
    };
    let mut bytes = Vec::new();
    write_frame(&mut bytes, &message).unwrap();
    let mut reader = Bytes::new(bytes);
    assert_eq!(
        read_frame::<_, ServerMessage>(&mut reader).unwrap(),
        Some(message)
    );
    assert_eq!(read_frame::<_, ServerMessage>(&mut reader).unwrap(), None);
}

#[test]
fn a_stream_ending_inside_a_frame_is_an_error() {
    let mut bytes = encode_frame(&ClientMessage::Command {
        command: Command::Cancel,
        id: CommandId::random(),
    })
    .unwrap();
    for cut in [2, 6] {
        bytes.truncate(cut);
        let error = read_frame::<_, ClientMessage>(&mut Bytes::new(bytes.clone())).unwrap_err();
        assert!(
            matches!(&error, Error::Io(io) if io.kind() == io::ErrorKind::UnexpectedEof),
            "{error:?}"
        );
    }
}

#[test]
fn oversized_and_empty_frames_are_refused_before_reading_the_payload() {
    let len = u32::try_from(MAX_FRAME_SIZE + 1).unwrap();
    let error = read_frame::<_, ClientMessage>(&mut Bytes::new(len.to_le_bytes())).unwrap_err();
    assert!(matches!(error, Error::FrameLimit { len } if len == MAX_FRAME_SIZE + 1));
    let error = read_frame::<_, ClientMessage>(&mut Bytes::new(0_u32.to_le_bytes())).unwrap_err();
    assert!(matches!(error, Error::EmptyFrame));
}

#[test]
fn malformed_payloads_keep_their_json_source() {
    let payload = b"{\"type\":\"nope\"}";
    let mut bytes = u32::try_from(payload.len()).unwrap().to_le_bytes().to_vec();
    bytes.extend_from_slice(payload);
    let error = read_frame::<_, ClientMessage>(&mut Bytes::new(bytes)).unwrap_err();
    assert!(matches!(error, Error::Json(_)));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn reads_survive_short_reads() {
    struct Trickle(Vec<u8>, usize);
    impl Read for Trickle {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let Some(byte) = self.0.get(self.1) else {
                return Ok(0);
            };
            buffer[0] = *byte;
            self.1 += 1;
            Ok(1)
        }
    }
    let message = ClientMessage::Hello {
        client: ClientKind::Gui,
        revision: REVISION,
        since: None,
        version: VERSION,
    };
    let bytes = encode_frame(&message).unwrap();
    assert_eq!(
        read_frame::<_, ClientMessage>(&mut Trickle(bytes, 0)).unwrap(),
        Some(message)
    );
}
