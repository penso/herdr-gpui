//! The relay's wire format: a tag byte, a big-endian `u32` length, then at
//! most `MAX_PAYLOAD` bytes. Both ends are this executable, so there is no
//! version negotiation; anything malformed ends the session.
use super::Error;
use crate::Result;
use std::io::{self, Read, Write};

/// The most bytes one frame carries; larger terminal I/O is split.
pub(super) const MAX_PAYLOAD: usize = 16 * 1024;
/// Longest `TERM` value passed on; real ones are a few dozen bytes.
const MAX_TERM: usize = 64;

/// A terminal size in cells.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Size {
    pub(super) rows: u16,
    pub(super) cols: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Frame {
    /// The relay's first frame: its terminal's size and `TERM`.
    Hello { size: Size, term: Option<String> },
    /// Keys typed in the pane.
    Input(Vec<u8>),
    /// The pane's terminal changed size.
    Resize(Size),
    /// What the setup printed.
    Output(Vec<u8>),
    /// The setup ended with this status; nothing follows.
    Exit(i32),
}

const HELLO: u8 = 1;
const INPUT: u8 = 2;
const RESIZE: u8 = 3;
const OUTPUT: u8 = 4;
const EXIT: u8 = 5;

impl Frame {
    /// Writes the frame in one call, so frames from several threads sharing a
    /// locked writer never interleave.
    pub(super) fn write(&self, writer: &mut impl Write) -> io::Result<()> {
        let (tag, payload) = match self {
            Self::Hello { size, term } => {
                let mut payload = size_bytes(*size).to_vec();
                payload.extend(term.as_deref().unwrap_or_default().bytes());
                (HELLO, payload)
            }
            Self::Input(bytes) => (INPUT, bytes.clone()),
            Self::Resize(size) => (RESIZE, size_bytes(*size).to_vec()),
            Self::Output(bytes) => (OUTPUT, bytes.clone()),
            Self::Exit(code) => (EXIT, code.to_be_bytes().to_vec()),
        };
        debug_assert!(payload.len() <= MAX_PAYLOAD);
        let mut frame = Vec::with_capacity(5 + payload.len());
        frame.push(tag);
        frame.extend((payload.len() as u32).to_be_bytes());
        frame.extend(payload);
        writer.write_all(&frame)?;
        writer.flush()
    }

    /// The next frame, or `None` when the peer closed between frames.
    pub(super) fn read(reader: &mut impl Read) -> Result<Option<Self>> {
        let mut header = [0u8; 5];
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                return Self::read(reader);
            }
            Err(error) => return Err(error.into()),
        }
        reader.read_exact(&mut header[1..])?;
        let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
        if length > MAX_PAYLOAD {
            return Err(Error::Frame.into());
        }
        let mut payload = vec![0u8; length];
        reader.read_exact(&mut payload)?;
        let frame = match header[0] {
            HELLO => {
                let size = size_from(payload.get(..4))?;
                let term = &payload[4..];
                // `TERM` names a terminfo entry; anything else is dropped.
                let term = (term.len() <= MAX_TERM
                    && !term.is_empty()
                    && term
                        .iter()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.+".contains(b)))
                .then(|| String::from_utf8_lossy(term).into_owned());
                Self::Hello { size, term }
            }
            INPUT => Self::Input(payload),
            RESIZE if payload.len() == 4 => Self::Resize(size_from(Some(&payload))?),
            OUTPUT => Self::Output(payload),
            EXIT if payload.len() == 4 => Self::Exit(i32::from_be_bytes([
                payload[0], payload[1], payload[2], payload[3],
            ])),
            _ => return Err(Error::Frame.into()),
        };
        Ok(Some(frame))
    }
}

fn size_bytes(size: Size) -> [u8; 4] {
    let [r0, r1] = size.rows.to_be_bytes();
    let [c0, c1] = size.cols.to_be_bytes();
    [r0, r1, c0, c1]
}

fn size_from(bytes: Option<&[u8]>) -> Result<Size> {
    match bytes {
        Some([r0, r1, c0, c1]) => Ok(Size {
            rows: u16::from_be_bytes([*r0, *r1]),
            cols: u16::from_be_bytes([*c0, *c1]),
        }),
        _ => Err(Error::Frame.into()),
    }
}
