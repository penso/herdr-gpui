//! One JSON document per line, bounded both ways, whatever carries it.
use super::*;

#[test]
fn a_line_is_read_without_its_newline_and_nothing_after_it() {
    let line = read_line(&b"{\"a\":1}\nrest"[..]).unwrap();
    assert_eq!(line, b"{\"a\":1}");
}

#[test]
fn a_line_of_exactly_the_limit_is_read() {
    let mut bytes = vec![b'x'; MAX_MESSAGE];
    bytes.push(b'\n');
    assert_eq!(read_line(&bytes[..]).unwrap().len(), MAX_MESSAGE);
}

#[test]
fn a_line_over_the_limit_is_refused_without_reading_further() {
    // Endless input: the bound, not EOF, has to stop the read.
    let endless = std::io::repeat(b'x');
    assert!(matches!(
        read_line(endless),
        Err(crate::Error::Control(crate::control::Error::MessageSize { limit })) if limit == MAX_MESSAGE
    ));
}

#[test]
fn a_peer_that_closes_early_leaves_what_it_sent() {
    // The caller then reports an empty line as no answer.
    assert_eq!(read_line(&b"partial"[..]).unwrap(), b"partial");
    assert!(read_line(&b""[..]).unwrap().is_empty());
}

#[test]
fn a_written_message_is_one_line_the_reader_takes_back() {
    let mut written = Vec::new();
    write_line(&mut written, &open("https://a.test/")).unwrap();
    assert_eq!(written.last(), Some(&b'\n'));
    assert_eq!(written.iter().filter(|&&byte| byte == b'\n').count(), 1);
    let request: Request = serde_json::from_slice(&read_line(&written[..]).unwrap()).unwrap();
    assert_eq!(request, open("https://a.test/"));
}

#[test]
fn an_answer_must_leave_room_for_its_newline() {
    assert!(fits(&Response::Feedback {
        text: Some("n".into()),
    }));
    let overhead = serde_json::to_vec(&Response::Feedback {
        text: Some(String::new()),
    })
    .unwrap()
    .len();
    let text = |len| Response::Feedback {
        text: Some("n".repeat(len)),
    };
    assert!(fits(&text(MAX_MESSAGE - overhead - 1)));
    assert!(!fits(&text(MAX_MESSAGE - overhead)));
}
