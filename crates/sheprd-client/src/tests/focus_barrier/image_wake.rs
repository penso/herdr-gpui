use super::*;

/// A partial outbound image defers the resend of a refused activation. Once
/// the image ends, the resend must not wait for an unrelated wake or deadline.
#[test]
fn an_activation_deferred_by_an_image_is_resent_when_the_image_ends() {
    let (client, mut server, worker) = connected();
    let focus = client.handle.focus_pane("boot-v1", "w1:p2").unwrap();
    let activation = client.handle.set_surface_active("boot-v1", true).unwrap();
    request(&mut server);
    let original = request(&mut server);
    client
        .handle
        .reserve_clipboard_image("boot-v1", ClientClipboardImageTarget::Pane("w1:p1".into()))
        .unwrap()
        .complete("png", vec![7; MAX_CLIPBOARD_IMAGE_PAYLOAD])
        .unwrap();
    let mut prefix = [0; 4];
    server.read_exact(&mut prefix).unwrap();
    // The image is partly written and backpressured; inbound reads continue.
    respond(
        &mut server,
        "boot-v1",
        &activation,
        json!({"error": {"code": "endpoint_busy", "message": "busy"}}),
    );
    respond(&mut server, "boot-v1", &focus, json!({"result": {}}));
    assert_eq!(response(&client).0, focus);
    let mut payload = vec![0; u32::from_le_bytes(prefix) as usize];
    server.read_exact(&mut payload).unwrap();
    assert!(matches!(
        decode_payload::<ClientMessage>(&payload).unwrap(),
        ClientMessage::ClipboardImage { .. }
    ));
    // Shorter than the 5-second idle cap a stranded resend would wait out.
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    assert_eq!(request(&mut server), original);
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}
