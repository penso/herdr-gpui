//! The few protobuf messages of boxd's public API this app sends and reads,
//! encoded by hand (`proto/api/v1/api.proto` in boxd's repository is the
//! contract), and the grpc-web frames around them. Only the fields used here
//! are written; every other field a reply carries is skipped, so a newer
//! server's additions are ignored rather than refused.

use super::{Error, Result};

/// A grpc-web frame's flag for a trailer block rather than a message.
pub const TRAILER: u8 = 0x80;
/// The largest frame accepted, as boxd's own clients cap a message.
pub const FRAME_LIMIT: usize = 4 * 1024 * 1024;

const VARINT: u8 = 0;
const FIXED64: u8 = 1;
const LENGTH: u8 = 2;
const FIXED32: u8 = 5;

/// A message being written, field by field.
#[derive(Default)]
pub struct Message(Vec<u8>);

impl Message {
    fn key(&mut self, field: u32, kind: u8) {
        varint(&mut self.0, u64::from(field) << 3 | u64::from(kind));
    }

    /// A length-delimited field; proto3 leaves an empty one out.
    pub fn bytes(mut self, field: u32, value: &[u8]) -> Self {
        if !value.is_empty() {
            self.key(field, LENGTH);
            varint(&mut self.0, value.len() as u64);
            self.0.extend_from_slice(value);
        }
        self
    }

    pub fn string(self, field: u32, value: &str) -> Self {
        self.bytes(field, value.as_bytes())
    }

    pub fn bool(mut self, field: u32, value: bool) -> Self {
        if value {
            self.key(field, VARINT);
            varint(&mut self.0, 1);
        }
        self
    }

    /// One entry of a `map<string, string>` field.
    pub fn entry(mut self, field: u32, key: &str, value: &str) -> Self {
        let entry = Message::default().string(1, key).string(2, value).0;
        self.key(field, LENGTH);
        varint(&mut self.0, entry.len() as u64);
        self.0.extend_from_slice(&entry);
        self
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// One field read from a message.
#[derive(Debug, PartialEq, Eq)]
pub enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    /// A fixed-width number; nothing read here has one.
    Fixed,
}

fn read_varint(bytes: &[u8], at: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at).ok_or(Error::Wire)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(Error::Wire)
}

/// Every `(field number, value)` in `bytes`, in order. A truncated or
/// malformed message is [`Error::Wire`].
pub fn fields(bytes: &[u8]) -> Result<Vec<(u32, Field<'_>)>> {
    let mut at = 0;
    let mut fields = Vec::new();
    while at < bytes.len() {
        let key = read_varint(bytes, &mut at)?;
        let number = u32::try_from(key >> 3).map_err(|_| Error::Wire)?;
        let field = match (key & 7) as u8 {
            VARINT => Field::Varint(read_varint(bytes, &mut at)?),
            LENGTH => {
                let length =
                    usize::try_from(read_varint(bytes, &mut at)?).map_err(|_| Error::Wire)?;
                let end = at.checked_add(length).ok_or(Error::Wire)?;
                let value = bytes.get(at..end).ok_or(Error::Wire)?;
                at = end;
                Field::Bytes(value)
            }
            FIXED64 => {
                at = at
                    .checked_add(8)
                    .filter(|end| *end <= bytes.len())
                    .ok_or(Error::Wire)?;
                Field::Fixed
            }
            FIXED32 => {
                at = at
                    .checked_add(4)
                    .filter(|end| *end <= bytes.len())
                    .ok_or(Error::Wire)?;
                Field::Fixed
            }
            _ => return Err(Error::Wire),
        };
        fields.push((number, field));
    }
    Ok(fields)
}

fn text(value: &[u8]) -> Result<String> {
    String::from_utf8(value.to_vec()).map_err(|_| Error::Wire)
}

/// A machine as `GetVm`, `ListVms`, and `CreateVm` describe it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Machine {
    pub id: String,
    pub name: String,
    /// Such as `running`, `suspended`, `hibernated`, or `stopped`.
    pub status: String,
}

impl Machine {
    /// A `GetVmResponse`.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut machine = Self::default();
        for (number, field) in fields(bytes)? {
            match (number, field) {
                (1, Field::Bytes(value)) => machine.id = text(value)?,
                (2, Field::Bytes(value)) => machine.name = text(value)?,
                (5, Field::Bytes(value)) => machine.status = text(value)?,
                _ => {}
            }
        }
        Ok(machine)
    }

    /// A `CreateVmResponse`, whose status is field 6 rather than 5.
    pub fn decode_created(bytes: &[u8]) -> Result<Self> {
        let mut machine = Self::default();
        for (number, field) in fields(bytes)? {
            match (number, field) {
                (1, Field::Bytes(value)) => machine.id = text(value)?,
                (2, Field::Bytes(value)) => machine.name = text(value)?,
                (6, Field::Bytes(value)) => machine.status = text(value)?,
                _ => {}
            }
        }
        Ok(machine)
    }

    /// The machines of a `ListVmsResponse`.
    pub fn decode_list(bytes: &[u8]) -> Result<Vec<Self>> {
        fields(bytes)?
            .into_iter()
            .filter_map(|(number, field)| match (number, field) {
                (1, Field::Bytes(value)) => Some(Self::decode(value)),
                _ => None,
            })
            .collect()
    }
}

/// One `ExecChunk` from the server: output, or the command's exit code.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub data: Vec<u8>,
    pub stderr: bool,
    /// Set on the chunk that ends the stream; proto3 leaves out a zero.
    pub exit_code: Option<i32>,
}

impl Output {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut output = Self::default();
        for (number, field) in fields(bytes)? {
            match (number, field) {
                (1, Field::Bytes(value)) => output.data = value.to_vec(),
                // An int32: a negative one travels as ten bytes of two's complement.
                (6, Field::Varint(value)) => output.exit_code = Some(value as i64 as i32),
                (10, Field::Varint(value)) => output.stderr = value != 0,
                _ => {}
            }
        }
        Ok(output)
    }
}

/// The first `ExecChunk`: which machine, and the command for its shell.
pub fn exec_start(machine: &str, command: &str) -> Vec<u8> {
    Message::default()
        .string(4, command)
        .string(5, machine)
        .into_bytes()
}

/// An `ExecChunk` carrying stdin bytes.
pub fn exec_stdin(data: &[u8]) -> Vec<u8> {
    Message::default().bytes(1, data).bool(2, true).into_bytes()
}

/// `message` as one grpc-web data frame.
pub fn frame(message: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(5 + message.len());
    frame.push(0);
    frame.extend_from_slice(&(message.len() as u32).to_be_bytes());
    frame.extend_from_slice(message);
    frame
}

/// The next whole frame at the start of `bytes`: its flag, its payload, and
/// how many bytes it took, or `None` until all of it has arrived.
pub fn take_frame(bytes: &[u8]) -> Result<Option<(u8, &[u8], usize)>> {
    let Some(header) = bytes.get(..5) else {
        return Ok(None);
    };
    let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
    if length > FRAME_LIMIT {
        return Err(Error::Size);
    }
    Ok(bytes
        .get(5..5 + length)
        .map(|payload| (header[0], payload, 5 + length)))
}

/// The gRPC status in a trailer block or response headers.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Trailers {
    pub status: Option<u32>,
    pub message: Option<String>,
}

impl Trailers {
    /// Read one `name: value` line; other names are ignored.
    pub fn header(&mut self, name: &str, value: &str) {
        if name.eq_ignore_ascii_case("grpc-status") {
            self.status = value.trim().parse().ok();
        } else if name.eq_ignore_ascii_case("grpc-message") {
            self.message = Some(
                percent_encoding::percent_decode_str(value.trim())
                    .decode_utf8_lossy()
                    .into_owned(),
            );
        }
    }

    /// A trailer frame's `name: value` lines.
    pub fn parse(block: &[u8]) -> Self {
        let mut trailers = Self::default();
        for line in String::from_utf8_lossy(block).split("\r\n") {
            if let Some((name, value)) = line.split_once(':') {
                trailers.header(name, value);
            }
        }
        trailers
    }
}

#[cfg(test)]
mod tests;
