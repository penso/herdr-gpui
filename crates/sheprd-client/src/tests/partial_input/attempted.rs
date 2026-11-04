use super::*;

/// A bounded write the full socket refused took no bytes. Room in the socket
/// must still wake its retry while it may go, and must not while it is fenced.
#[test]
fn a_refused_input_wakes_on_room_only_while_it_may_be_written() {
    let session = settled();
    let mut input = command(pane_input(vec![key()]));
    input.written.started = Some(Instant::now());
    assert!(input.input_attempted() && !input.started());
    let current = partial(ServerMessage::PaneSurface(next_surface()));
    assert!(session.input_writable(&input, &current));
    assert!(session.input_writable(&input, &FrameReader::new()));
    let unknown = partial(ServerMessage::EndpointControl {
        kind: ENDPOINT_SNAPSHOT_KIND.into(),
        data: SNAPSHOT.into(),
    });
    assert!(!session.input_writable(&input, &unknown));
    // Bytes on the wire must finish whatever frame is arriving.
    input.written.offset = 1;
    assert!(session.input_writable(&input, &unknown));
}
