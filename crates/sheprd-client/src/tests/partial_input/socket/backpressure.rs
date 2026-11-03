use super::*;

fn text() -> ClientPaneInputEvent {
    ClientPaneInputEvent::TextCommit("p".repeat(3000))
}

/// Input permitted beside a held surface can fill the socket while the peer is
/// still sending. The worker must keep reading rather than block on the write
/// past its timeout, and every input frame must arrive whole and in order.
#[test]
fn input_that_fills_the_socket_waits_for_room_without_blocking_reads() {
    let (client, mut server, observer) = peer();
    let mut surface = large_surface();
    surface.surface_revision = 2;
    let tail = fragment(&mut server, &observer, ServerMessage::PaneSurface(surface));
    for _ in 0..64 {
        client
            .handle
            .send_input("boot-v1", "w1:p1", [text()])
            .unwrap();
    }
    // Longer than the one-second timeout of an ordinary blocking write.
    thread::sleep(Duration::from_millis(1500));
    assert!(
        client.events.try_recv().is_err(),
        "the connection failed while the peer was busy"
    );
    server.write_all(&tail).unwrap();
    assert!(
        matches!(event(&client), ClientEvent::Surface(surface) if surface.surface_revision == 2)
    );
    for _ in 0..64 {
        assert_eq!(receive(&mut server), pane_input(vec![text()]));
    }
    close(client, server, observer);
}

/// A peer that reads while it holds its tail receives all of the input before
/// releasing it: room in the socket, not the end of the frame, resumes writes.
#[test]
fn input_flows_while_the_peer_reads_during_a_held_tail() {
    let (client, mut server, observer) = peer();
    let mut surface = large_surface();
    surface.surface_revision = 2;
    let tail = fragment(&mut server, &observer, ServerMessage::PaneSurface(surface));
    for _ in 0..64 {
        client
            .handle
            .send_input("boot-v1", "w1:p1", [text()])
            .unwrap();
    }
    // Shorter than the 5-second idle cap a stranded write would wait out.
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    for _ in 0..64 {
        assert_eq!(receive(&mut server), pane_input(vec![text()]));
    }
    assert!(
        client.events.try_recv().is_err(),
        "partial surface was published"
    );
    server.write_all(&tail).unwrap();
    assert!(
        matches!(event(&client), ClientEvent::Surface(surface) if surface.surface_revision == 2)
    );
    close(client, server, observer);
}
