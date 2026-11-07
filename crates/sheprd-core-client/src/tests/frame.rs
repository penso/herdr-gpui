use crate::frame::{Decoder, Error, Frame, HEADER, Kind, MAX_PAYLOAD, encode};

#[test]
fn a_frame_round_trips_through_any_split() {
    let bytes = [
        encode(7, Kind::Data, b"hello").unwrap(),
        encode(0, Kind::Credit, &5u32.to_le_bytes()).unwrap(),
    ]
    .concat();
    for split in 0..bytes.len() {
        let mut decoder = Decoder::default();
        decoder.push(&bytes[..split]);
        let mut frames = Vec::new();
        while let Some(frame) = decoder.next_frame().unwrap() {
            frames.push(frame);
        }
        decoder.push(&bytes[split..]);
        while let Some(frame) = decoder.next_frame().unwrap() {
            frames.push(frame);
        }
        assert_eq!(
            frames,
            [
                Frame {
                    channel: 7,
                    kind: Kind::Data,
                    payload: b"hello".to_vec()
                },
                Frame {
                    channel: 0,
                    kind: Kind::Credit,
                    payload: 5u32.to_le_bytes().to_vec()
                },
            ],
            "split at {split}"
        );
        assert_eq!(decoder.pending(), 0);
    }
}

#[test]
fn an_oversized_header_fails_before_its_payload_arrives() {
    let mut header = vec![0; HEADER];
    header[5..9].copy_from_slice(&(MAX_PAYLOAD as u32 + 1).to_le_bytes());
    let mut decoder = Decoder::default();
    decoder.push(&header);
    assert_eq!(decoder.next_frame(), Err(Error::TooLarge(MAX_PAYLOAD + 1)));
    assert_eq!(
        encode(1, Kind::Data, &vec![0; MAX_PAYLOAD + 1]),
        Err(Error::TooLarge(MAX_PAYLOAD + 1))
    );
}

#[test]
fn an_unknown_kind_is_refused() {
    let mut frame = encode(1, Kind::Data, b"x").unwrap();
    frame[4] = 9;
    let mut decoder = Decoder::default();
    decoder.push(&frame);
    assert_eq!(decoder.next_frame(), Err(Error::UnknownKind(9)));
}

#[test]
fn kinds_keep_their_wire_numbers() {
    let kinds = [
        Kind::Data,
        Kind::Credit,
        Kind::Close,
        Kind::Resize,
        Kind::Focus,
        Kind::Facts,
    ];
    for (number, kind) in kinds.into_iter().enumerate() {
        assert_eq!(u8::from(kind), number as u8);
        assert_eq!(Kind::try_from(number as u8), Ok(kind));
    }
}
