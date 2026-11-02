//! How a message crosses the `pair` and `presence` characteristics: a
//! 2-byte big-endian length, then the message, cut into chunks a write or
//! notification carries. The same rules as the macOS helper
//! (`macos/herdr-nearby/Framing.swift`) and the iOS app (`NearbyFraming`).

/// The largest message either side accepts.
pub(crate) const MAX_MESSAGE: usize = 4096;
/// The smallest chunk: what any Bluetooth LE link carries.
pub(crate) const MIN_CHUNK: usize = 20;

/// `message`, framed and cut into chunks of at most `size` bytes (never
/// below [`MIN_CHUNK`]). A message over [`MAX_MESSAGE`] gives nothing.
pub(crate) fn chunks(message: &[u8], size: usize) -> Vec<Vec<u8>> {
    let Ok(length) = u16::try_from(message.len()) else {
        return Vec::new();
    };
    if message.len() > MAX_MESSAGE {
        return Vec::new();
    }
    let mut framed = Vec::with_capacity(message.len() + 2);
    framed.extend_from_slice(&length.to_be_bytes());
    framed.extend_from_slice(message);
    framed
        .chunks(size.max(MIN_CHUNK))
        .map(<[u8]>::to_vec)
        .collect()
}

/// Collects one message's chunks.
#[derive(Debug, Default)]
pub(crate) struct Reassembly {
    buffer: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Partial,
    Complete(Vec<u8>),
    /// Longer than its length says, or over [`MAX_MESSAGE`]: dropped.
    Invalid,
}

impl Reassembly {
    pub(crate) fn add(&mut self, chunk: &[u8]) -> Outcome {
        self.buffer.extend_from_slice(chunk);
        let [high, low, ..] = self.buffer[..] else {
            return Outcome::Partial;
        };
        let length = usize::from(u16::from_be_bytes([high, low]));
        if length > MAX_MESSAGE || self.buffer.len() > length + 2 {
            return Outcome::Invalid;
        }
        if self.buffer.len() == length + 2 {
            let message = self.buffer.split_off(2);
            self.buffer.clear();
            Outcome::Complete(message)
        } else {
            Outcome::Partial
        }
    }
}
