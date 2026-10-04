use super::*;

#[test]
fn fragmented_frames_survive_timeout_between_every_byte() {
    let (mut client, mut server) = Stream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_millis(1)))
        .unwrap();
    let expected = ServerMessage::TerminalBell { count: 300 };
    let bytes = encode_message(&expected, MAX_FRAME_SIZE).unwrap();
    let mut reader = FrameReader::new();
    let mut message = None;
    for byte in bytes {
        assert!(reader.poll(&mut client).unwrap().is_none()); // timeout, preserving state
        server.write_all(&[byte]).unwrap();
        if let Some(next) = reader.poll(&mut client).unwrap() {
            message = Some(next);
        }
    }
    assert_eq!(message, Some(expected));
}

#[test]
fn geometry_and_frame_reader_limits() {
    for (cols, rows, cell_width_px) in [(0, 24, 0), (4097, 1, 0), (1001, 1000, 0), (80, 24, 4097)] {
        assert!(
            validate_options(ConnectOptions {
                surface_size: ClientSurfaceSize { cols, rows },
                cell_width_px,
                cell_height_px: 0,
            })
            .is_err()
        );
    }
    let (mut client, mut server) = Stream::pair().unwrap();
    server
        .write_all(&((MAX_GRAPHICS_FRAME_SIZE + 1) as u32).to_le_bytes())
        .unwrap();
    assert!(FrameReader::new().poll(&mut client).is_err());
    let mut reader = FrameReader::new();
    reader.started = Some(Instant::now() - TIMEOUT - POLL);
    assert!(
        reader
            .poll(&mut client)
            .unwrap_err()
            .to_string()
            .contains("timed out")
    );
}

#[test]
fn response_boot_id_correlation_and_assembly_limits() {
    for case in ["boot", "id", "limit"] {
        let (client, mut server, worker) = test_client();
        handshake(&mut server);
        event(&client);
        event(&client);
        let id = client.handle.focus_pane("boot-v1", "w1:p1").unwrap();
        receive(&mut server);
        let data = match case {
            "limit" => vec![b' '; MAX_RESPONSE_BYTES + 1],
            "id" => br#"{"id":"wrong","result":{}}"#.to_vec(),
            _ => vec![],
        };
        send(
            &mut server,
            ServerMessage::ClientShellEndpointResponseChunk {
                boot_id: if case == "boot" { "stale" } else { "boot-v1" }.into(),
                request_id: id,
                final_chunk: true,
                data,
            },
        );
        let error = worker.join().unwrap().unwrap_err().to_string();
        assert!(
            error.contains(match case {
                "boot" => "boot mismatch",
                "id" => "ID mismatch",
                _ => "limit exceeded",
            }),
            "{error}"
        );
    }
}

#[test]
fn frame_reader_accepts_read_trait_objects_and_preserves_partial_state() {
    struct Fragmented {
        bytes: io::Cursor<Vec<u8>>,
        pause: bool,
        error: io::ErrorKind,
    }
    impl Read for Fragmented {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.pause = !self.pause;
            if self.pause {
                return Err(self.error.into());
            }
            let len = buf.len().min(1);
            self.bytes.read(&mut buf[..len])
        }
    }
    let expected = ServerMessage::TerminalBell { count: 300 };
    let bytes = encode_message(&expected, MAX_FRAME_SIZE).unwrap();
    for error in [
        io::ErrorKind::WouldBlock,
        io::ErrorKind::TimedOut,
        io::ErrorKind::Interrupted,
    ] {
        let mut input = Fragmented {
            bytes: io::Cursor::new(bytes.repeat(2)),
            pause: false,
            error,
        };
        let input: &mut dyn Read = &mut input;
        let mut reader = FrameReader::new();
        for _ in 0..2 {
            for index in 0..bytes.len() {
                assert!(reader.poll(input).unwrap().is_none());
                let message = reader.poll(input).unwrap();
                if index + 1 == bytes.len() {
                    assert_eq!(message, Some(expected.clone()));
                    assert!(reader.started.is_none());
                    assert!(reader.bytes.is_empty());
                    assert_eq!(reader.target, 4);
                } else {
                    assert!(message.is_none());
                }
            }
        }
        assert!(reader.poll(input).unwrap().is_none());
        assert_eq!(
            reader.poll(input).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
}
