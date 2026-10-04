use super::*;
use crate::frame::ImageWriter;

#[test]
fn read_batches_bound_progress_and_stop_on_the_first_idle_read() {
    struct Input {
        bytes: io::Cursor<Vec<u8>>,
        calls: usize,
        idle: Option<io::ErrorKind>,
    }
    impl Read for Input {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            if let Some(kind) = self.idle {
                return Err(kind.into());
            }
            self.bytes.read(bytes)
        }
    }
    let expected = ServerMessage::Graphics {
        bytes: vec![17; 2 * 1024 * 1024],
    };
    let mut input = Input {
        bytes: io::Cursor::new(encode_message(&expected, MAX_GRAPHICS_FRAME_SIZE).unwrap()),
        calls: 0,
        idle: None,
    };
    let mut reader = FrameReader::new();
    assert!(reader.poll_batch(&mut input).unwrap().is_none());
    assert!((1..=128).contains(&input.calls));
    assert!(reader.bytes.len() <= 1024 * 1024);
    let partial = reader.bytes.len();
    for kind in [
        io::ErrorKind::WouldBlock,
        io::ErrorKind::TimedOut,
        io::ErrorKind::Interrupted,
    ] {
        input.idle = Some(kind);
        input.calls = 0;
        assert!(reader.poll_batch(&mut input).unwrap().is_none());
        assert_eq!(input.calls, 1);
        assert_eq!(reader.bytes.len(), partial);
    }
    input.idle = None;
    loop {
        input.calls = 0;
        let message = reader.poll_batch(&mut input).unwrap();
        assert!(input.calls <= 128);
        if let Some(message) = message {
            assert_eq!(message, expected);
            break;
        }
    }
}

#[test]
fn image_writer_preserves_offsets_through_short_writes_and_timeouts() {
    struct ShortWriter {
        bytes: Vec<u8>,
        calls: usize,
    }
    impl Write for ShortWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            assert!(bytes.len() <= 64 * 1024);
            self.calls += 1;
            match self.calls % 4 {
                0 => Err(io::ErrorKind::TimedOut.into()),
                1 => Err(io::ErrorKind::Interrupted.into()),
                2 => Err(io::ErrorKind::WouldBlock.into()),
                _ => {
                    let n = bytes.len().min(997);
                    self.bytes.extend_from_slice(&bytes[..n]);
                    Ok(n)
                }
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("must not flush a partial image")
        }
    }
    let bytes = encode_clipboard_image(
        ClientClipboardImageTarget::Popup("popup".into()),
        "PNG",
        vec![42; MAX_FRAME_SIZE + 1],
    )
    .unwrap();
    let mut stream = ShortWriter {
        bytes: Vec::new(),
        calls: 0,
    };
    let mut writer = ImageWriter::default();
    for _ in 0..20_000 {
        if writer.poll(&mut stream, &bytes).unwrap() {
            break;
        }
    }
    assert_eq!(stream.bytes, bytes);
    writer.started = Some(Instant::now() - COMMAND_TIMEOUT);
    assert!(matches!(
        writer.check_timeout(),
        Err(Error::ClipboardImageWriteTimeout)
    ));
}

#[cfg(windows)]
#[test]
fn images_refuse_unbounded_pipe_writes() {
    let (client, _server, worker) = test_client();
    assert!(matches!(
        client
            .handle
            .reserve_clipboard_image("boot", ClientClipboardImageTarget::DirectTerminal),
        Err(Error::ClipboardImageUnsupported)
    ));
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[cfg(unix)]
mod unix;
