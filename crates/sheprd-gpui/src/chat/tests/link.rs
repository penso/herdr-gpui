use super::*;
use crate::chat::link::{Link, Polled};
use sheprd_chat_protocol::{ByeReason, Command, CommandId};

/// Polls `link` until it has `count` messages or ends, at most [`WAIT`].
fn collect(link: &Link, count: usize) -> Polled {
    let mut all = Polled::default();
    wait_until(|| {
        let polled = link.poll(usize::MAX);
        all.messages.extend(polled.messages);
        all.end = all.end.take().or(polled.end);
        all.messages.len() >= count || all.end.is_some()
    });
    all
}

fn command(command: Command) -> ClientMessage {
    ClientMessage::Command {
        command,
        id: CommandId::random(),
    }
}

#[test]
fn the_worker_greets_and_delivers_in_order() {
    let program = Program::new();
    let link = Link::start(program.path.clone(), Some(cursor(1, 7)), []).unwrap();
    let (mut stream, greeting) = program.accept();
    assert_eq!(greeting, hello(Some(cursor(1, 7))));
    send(&mut stream, &welcome());
    send(&mut stream, &snapshot(cursor(1, 7), vec![agent(1, "a")]));
    send(&mut stream, &change(append(1, "b"), cursor(1, 8)));
    let polled = collect(&link, 3);
    assert!(polled.end.is_none());
    assert_eq!(polled.messages[0], welcome());
    assert!(matches!(polled.messages[1], ServerMessage::Snapshot { .. }));
    assert!(matches!(
        polled.messages[2],
        ServerMessage::Change { cursor: at, .. } if at == cursor(1, 8)
    ));
}

#[test]
fn commands_follow_the_greeting_and_resent_ones_go_first() {
    let program = Program::new();
    let resent = command(Command::Prompt {
        text: "a".into(),
        attachments: Vec::new(),
    });
    let link = Link::start(program.path.clone(), None, [resent.clone()]).unwrap();
    let later = command(Command::Cancel);
    link.send(later.clone()).unwrap();
    let (mut stream, greeting) = program.accept();
    assert_eq!(greeting, hello(None));
    assert_eq!(read(&mut stream), Some(resent));
    assert_eq!(read(&mut stream), Some(later));
}

#[test]
fn a_closed_connection_ends_after_its_messages() {
    let program = Program::new();
    let link = Link::start(program.path.clone(), None, []).unwrap();
    let (mut stream, _) = program.accept();
    send(&mut stream, &welcome());
    send(
        &mut stream,
        &ServerMessage::Bye {
            reason: ByeReason::Exiting,
        },
    );
    drop(stream);
    let polled = collect(&link, usize::MAX);
    assert_eq!(polled.messages.len(), 2);
    assert!(matches!(polled.end, Some(Error::Closed)));
}

#[test]
fn an_unreachable_program_ends_the_link() {
    let (_dir, missing) = address("missing");
    let link = Link::start(missing, None, []).unwrap();
    let polled = collect(&link, usize::MAX);
    assert!(polled.messages.is_empty());
    assert!(matches!(polled.end, Some(Error::Connect(_))));
}

#[test]
fn dropping_the_link_closes_the_socket() {
    let program = Program::new();
    let link = Link::start(program.path.clone(), None, []).unwrap();
    let (mut stream, _) = program.accept();
    drop(link);
    // The program sees the end of the stream rather than waiting.
    assert_eq!(read(&mut stream), None);
}

#[test]
fn a_window_that_falls_behind_is_cut_off() {
    let program = Program::new();
    let link = Link::start(program.path.clone(), None, []).unwrap();
    let (mut stream, _) = program.accept();
    for _ in 0..MAX_EVENTS + 8 {
        if write_frame(&mut stream, &welcome()).is_err() {
            break;
        }
    }
    // The worker shuts the socket once it gives up; only then does the
    // window poll, so polling cannot keep it from falling behind.
    assert!(!matches!(
        read_frame::<_, ClientMessage>(&mut stream),
        Ok(Some(_))
    ));
    let mut messages = 0;
    let mut end = None;
    wait_until(|| {
        let polled = link.poll(usize::MAX);
        messages += polled.messages.len();
        end = polled.end;
        end.is_some()
    });
    assert!(matches!(end, Some(Error::Overflow)));
    assert!(messages <= MAX_EVENTS);
}

#[test]
fn a_frame_the_program_cannot_have_sent_ends_the_link() {
    let program = Program::new();
    let link = Link::start(program.path.clone(), None, []).unwrap();
    let (mut stream, _) = program.accept();
    stream.write_all(&[3, 0, 0, 0, b'{', b'x', b'}']).unwrap();
    let polled = collect(&link, usize::MAX);
    assert!(matches!(polled.end, Some(Error::Read(_))));
}

/// `bytes` as a POSIX `printf` format of octal escapes.
#[cfg(unix)]
fn printf_format(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("\\{byte:03o}")).collect()
}

#[cfg(unix)]
#[test]
fn a_child_process_carries_the_connection() {
    use crate::chat::link::Dial;
    let mut frame = Vec::new();
    write_frame(&mut frame, &welcome()).unwrap();
    // The child greets, then holds its stdout open until the link kills it.
    let mut command = std::process::Command::new("/bin/sh");
    command.args([
        "-c",
        &format!("printf '{}'; exec sleep 30", printf_format(&frame)),
    ]);
    let link = Link::start(Dial::Process(command), None, []).unwrap();
    let polled = collect(&link, 1);
    assert_eq!(polled.messages.first(), Some(&welcome()));
    assert!(polled.end.is_none(), "{:?}", polled.end);
    drop(link);
}

#[cfg(unix)]
#[test]
fn a_child_that_exits_ends_the_link() {
    use crate::chat::link::Dial;
    let mut command = std::process::Command::new("/bin/sh");
    command.args(["-c", "exit 127"]);
    let link = Link::start(Dial::Process(command), None, []).unwrap();
    let polled = collect(&link, 1);
    assert!(polled.messages.is_empty());
    assert!(matches!(polled.end, Some(Error::Closed)));
}
