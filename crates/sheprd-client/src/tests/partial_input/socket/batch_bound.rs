use super::*;

/// Input queued behind an outbound image becomes eligible all at once when
/// the image ends. More of it than one pass writes must not wait for the tail
/// of a held inbound surface: the wakes that queued it are already spent.
#[test]
fn input_past_the_batch_bound_arrives_before_a_held_surface_tail() {
    let (client, mut server, observer) = peer();
    client
        .handle
        .reserve_clipboard_image("boot-v1", ClientClipboardImageTarget::Pane("w1:p1".into()))
        .unwrap()
        .complete("png", vec![7; MAX_CLIPBOARD_IMAGE_PAYLOAD])
        .unwrap();
    let mut prefix = [0; 4];
    server.read_exact(&mut prefix).unwrap();
    // The image is partly written and backpressured; inbound reads continue.
    let mut surface = large_surface();
    surface.surface_revision = 2;
    let tail = fragment(&mut server, &observer, ServerMessage::PaneSurface(surface));
    for _ in 0..40 {
        client
            .handle
            .send_input("boot-v1", "w1:p1", [key()])
            .unwrap();
    }
    let mut payload = vec![0; u32::from_le_bytes(prefix) as usize];
    server.read_exact(&mut payload).unwrap();
    assert!(matches!(
        decode_payload::<ClientMessage>(&payload).unwrap(),
        ClientMessage::ClipboardImage { .. }
    ));
    // Shorter than the 5-second idle cap a stranded command would wait out.
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    for _ in 0..40 {
        assert_eq!(receive(&mut server), pane_input(vec![key()]));
    }
    server.write_all(&tail).unwrap();
    assert!(
        matches!(event(&client), ClientEvent::Surface(surface) if surface.surface_revision == 2)
    );
    close(client, server, observer);
}
