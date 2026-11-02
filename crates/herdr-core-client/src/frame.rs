//! Framed mode: `[channel u32 LE][kind u8][len u32 LE][payload]`.

/// Header bytes before each payload.
pub const HEADER: usize = 9;
/// The largest payload either side may send.
pub const MAX_PAYLOAD: usize = 16 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("a {0}-byte frame is over the {MAX_PAYLOAD}-byte limit")]
    TooLarge(usize),
    #[error("unknown frame kind {0}")]
    UnknownKind(u8),
}

/// The wire order is the protocol's: never reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Channel 0: one JSON-RPC message. Other channels: stream bytes.
    Data,
    /// u32 LE: bytes the receiver consumed.
    Credit,
    /// The channel is over; from the core, a JSON reason.
    Close,
    /// Viewer to core: u16 LE cols, u16 LE rows.
    Resize,
    /// Viewer to core: 1 gained focus, 0 lost it.
    Focus,
    /// Viewer to core: JSON display facts.
    Facts,
}

impl From<Kind> for u8 {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Data => 0,
            Kind::Credit => 1,
            Kind::Close => 2,
            Kind::Resize => 3,
            Kind::Focus => 4,
            Kind::Facts => 5,
        }
    }
}

impl TryFrom<u8> for Kind {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Error> {
        Ok(match value {
            0 => Kind::Data,
            1 => Kind::Credit,
            2 => Kind::Close,
            3 => Kind::Resize,
            4 => Kind::Focus,
            5 => Kind::Facts,
            other => return Err(Error::UnknownKind(other)),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub channel: u32,
    pub kind: Kind,
    pub payload: Vec<u8>,
}

pub fn encode(channel: u32, kind: Kind, payload: &[u8]) -> Result<Vec<u8>, Error> {
    let len = u32::try_from(payload.len())
        .ok()
        .filter(|_| payload.len() <= MAX_PAYLOAD)
        .ok_or(Error::TooLarge(payload.len()))?;
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(&channel.to_le_bytes());
    out.push(kind.into());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Collects bytes as they arrive and yields whole frames, so a frame split
/// across reads (or a read timeout) is finished by later bytes.
#[derive(Debug, Default)]
pub struct Decoder {
    buffer: Vec<u8>,
    start: usize,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        if self.start > 0 && self.start >= self.buffer.len() / 2 {
            self.buffer.drain(..self.start);
            self.start = 0;
        }
        self.buffer.extend_from_slice(bytes);
    }

    /// The next whole frame, or none until more bytes arrive. A bad header
    /// is an error before its payload is buffered.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, Error> {
        let ready = &self.buffer[self.start..];
        let Some(header) = ready.get(..HEADER) else {
            return Ok(None);
        };
        let channel = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let kind = Kind::try_from(header[4])?;
        let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
        if len > MAX_PAYLOAD {
            return Err(Error::TooLarge(len));
        }
        let Some(payload) = ready.get(HEADER..HEADER + len) else {
            return Ok(None);
        };
        let frame = Frame {
            channel,
            kind,
            payload: payload.to_vec(),
        };
        self.start += HEADER + len;
        Ok(Some(frame))
    }

    /// Bytes received but not yet part of a whole frame.
    pub fn pending(&self) -> usize {
        self.buffer.len() - self.start
    }
}
