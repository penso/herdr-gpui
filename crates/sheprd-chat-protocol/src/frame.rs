//! Length-prefixed JSON frames.

use std::io::{self, Read, Write};

use serde::{Serialize, de::DeserializeOwned};

use crate::{Error, Result};

/// The largest frame either side writes or reads, in bytes.
pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

pub fn encode_frame<M: Serialize>(message: &M) -> Result<Vec<u8>> {
    let mut bytes = vec![0; 4];
    serde_json::to_writer(&mut bytes, message)?;
    let len = bytes.len() - 4;
    if len > MAX_FRAME_SIZE {
        return Err(Error::FrameLimit { len });
    }
    let prefix = u32::try_from(len).map_err(|_| Error::FrameLimit { len })?;
    bytes[..4].copy_from_slice(&prefix.to_le_bytes());
    Ok(bytes)
}

pub fn write_frame<W: Write, M: Serialize>(writer: &mut W, message: &M) -> Result<()> {
    writer.write_all(&encode_frame(message)?)?;
    Ok(writer.flush()?)
}

/// Reads one frame, or `None` when the peer closed the stream between frames.
/// A stream that ends inside a frame is an error.
pub fn read_frame<R: Read, M: DeserializeOwned>(reader: &mut R) -> Result<Option<M>> {
    let mut prefix = [0; 4];
    let mut filled = 0;
    while filled < prefix.len() {
        match reader.read(&mut prefix[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    let len = u32::from_le_bytes(prefix) as usize;
    if len == 0 {
        return Err(Error::EmptyFrame);
    }
    if len > MAX_FRAME_SIZE {
        return Err(Error::FrameLimit { len });
    }
    let mut payload = vec![0; len];
    reader.read_exact(&mut payload)?;
    Ok(Some(serde_json::from_slice(&payload)?))
}
