use super::*;

#[test]
fn named_pipe_keeps_current_pane_input_behind_a_partial_surface() {
    let mut session = ready_session();
    let mut surface = baseline();
    let rect = SurfaceRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    surface.panes.push(PaneSurfacePane {
        pane_id: "w1:p1".into(),
        content_revision: 1,
        rect,
        inner_rect: rect,
        scrollbar_rect: None,
        scroll: None,
        focused: true,
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        alternate_screen_active: false,
        pixel_width: 0,
        pixel_height: 0,
    });
    session
        .handle_message(ServerMessage::PaneSurface(surface.clone()), |_| Ok(()))
        .unwrap();
    surface.surface_revision += 1;
    let bytes = encode_message(
        &ServerMessage::PaneSurface(surface),
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
    let mut reader = FrameReader::new();
    let mut fragment = io::Cursor::new(&bytes[..bytes.len() - 1]);
    while reader.bytes.len() < bytes.len() - 1 {
        assert!(reader.poll(&mut fragment).unwrap().is_none());
    }
    let input = ClientMessage::ClientShellPaneInput {
        pane_id: "w1:p1".into(),
        events: vec![ClientPaneInputEvent::TextCommit("public".into())],
    };
    let command = handle::Command {
        boot_id: "boot-v1".into(),
        bytes: encode_message(&input, MAX_FRAME_SIZE).unwrap(),
        input_pane: Some("w1:p1".into()),
        written: Default::default(),
        request: None,
        image: None,
    };
    assert!(reader.has_current_surface_prefix(
        session.snapshot.as_ref().unwrap(),
        session.surface.as_ref().unwrap()
    ));
    assert!(!session.permits_partial_input(&command, &reader));
}
