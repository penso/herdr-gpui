use crate::framing::{MAX_MESSAGE, MIN_CHUNK, Outcome, Reassembly, chunks};

#[test]
fn a_message_is_framed_with_its_length_and_cut_to_the_link() {
    let message = vec![7u8; 50];
    let cut = chunks(&message, 22);
    assert_eq!(cut.len(), 3);
    assert_eq!(&cut[0][..2], &[0, 50]);
    assert!(cut.iter().all(|chunk| chunk.len() <= 22));
    assert_eq!(cut.concat()[2..], message[..]);
}

#[test]
fn chunks_are_never_below_what_any_link_carries() {
    assert!(
        chunks(&[1; 40], 3)
            .iter()
            .all(|chunk| chunk.len() == MIN_CHUNK || chunk.len() == 42 - MIN_CHUNK * 2)
    );
    assert_eq!(chunks(&[1; 40], 3)[0].len(), MIN_CHUNK);
}

#[test]
fn an_oversized_message_is_not_sent() {
    assert!(chunks(&vec![0; MAX_MESSAGE + 1], 100).is_empty());
    assert!(!chunks(&vec![0; MAX_MESSAGE], 100).is_empty());
}

#[test]
fn chunks_reassemble_into_the_message() {
    let message = br#"{"type":"hello","hint":"abc"}"#;
    let mut reassembly = Reassembly::default();
    let mut outcome = Outcome::Partial;
    for chunk in chunks(message, 20) {
        outcome = reassembly.add(&chunk);
    }
    assert_eq!(outcome, Outcome::Complete(message.to_vec()));
    // The next message starts clean.
    assert_eq!(reassembly.add(&[0, 1]), Outcome::Partial);
    assert_eq!(reassembly.add(b"x"), Outcome::Complete(b"x".to_vec()));
}

#[test]
fn a_length_over_the_limit_or_extra_bytes_are_invalid() {
    let too_long = u16::try_from(MAX_MESSAGE + 1).unwrap().to_be_bytes();
    assert_eq!(Reassembly::default().add(&too_long), Outcome::Invalid);
    assert_eq!(
        Reassembly::default().add(&[0, 1, b'a', b'b']),
        Outcome::Invalid
    );
    assert_eq!(Reassembly::default().add(&[0]), Outcome::Partial);
}
