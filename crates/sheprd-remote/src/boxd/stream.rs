//! boxd's exec stream on the wire, without a socket: the request head and
//! chunks this app sends, and a decoder for the reply that takes bytes as
//! they arrive, in any fragments, and yields the command's output, its exit
//! code, and the gRPC status that ends the call.
//!
//! `Exec` is a bidirectional gRPC stream. boxd serves it as grpc-web over
//! HTTP/1.1 too, and runs it full duplex: the reply starts while the request
//! body is still open, so stdin is a chunked request body that ends when
//! this process's stdin does, and output arrives as the reply's chunks.

use super::{
    Error, Result, Settings,
    error::{Code, Status},
    http::{CONTENT_TYPE, SERVICE},
    wire::{self, Output, TRAILER, Trailers},
};
use secrecy::{ExposeSecret, SecretString};
use std::io::Write;
use zeroize::Zeroizing;

const HEAD_LIMIT: usize = 64 * 1024;
const SIZE_LINE_LIMIT: usize = 1024;
const HEADERS: usize = 64;
/// The chunk that ends a chunked request body.
pub const END: &[u8] = b"0\r\n\r\n";

/// The request head and the first message, which names the machine and the
/// command. Holds the API key, so it is wiped when dropped.
pub fn open(
    settings: &Settings,
    key: &SecretString,
    machine: &str,
    command: &str,
) -> Zeroizing<Vec<u8>> {
    let mut head = Zeroizing::new(Vec::with_capacity(512));
    // Writing to a Vec cannot fail.
    let _ = write!(
        head,
        "POST {SERVICE}Exec HTTP/1.1\r\nHost: {}\r\nContent-Type: {CONTENT_TYPE}\r\nAccept: {CONTENT_TYPE}\r\nX-Grpc-Web: 1\r\nUser-Agent: Sheprd\r\nTransfer-Encoding: chunked\r\nAuthorization: Bearer ",
        settings.authority()
    );
    head.extend_from_slice(key.expose_secret().as_bytes());
    head.extend_from_slice(b"\r\n\r\n");
    chunk(&mut head, &wire::frame(&wire::exec_start(machine, command)));
    head
}

/// `data` as the next stdin chunk of the request body.
pub fn stdin(out: &mut Vec<u8>, data: &[u8]) {
    chunk(out, &wire::frame(&wire::exec_stdin(data)));
}

fn chunk(out: &mut Vec<u8>, data: &[u8]) {
    let _ = write!(out, "{:x}\r\n", data.len());
    out.extend_from_slice(data);
    out.extend_from_slice(b"\r\n");
}

/// What the reply said, in order.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(i32),
    /// The call ended with this status; nothing follows.
    End(Trailers),
}

#[derive(Debug)]
enum State {
    Head,
    /// A chunk-size line is next.
    Size,
    /// This many bytes of the current chunk are still to come.
    Data(usize),
    /// The CRLF after a chunk.
    DataEnd,
    /// A body of known length.
    Length(usize),
    /// A body that runs to the connection's end.
    Close,
    Done,
}

/// Decodes the reply incrementally; a fragment that ends mid-head, mid-chunk,
/// or mid-frame waits for the rest.
#[derive(Debug)]
pub struct Decoder {
    state: State,
    /// Bytes not yet parsed at the HTTP layer.
    raw: Vec<u8>,
    /// De-chunked body bytes not yet a whole grpc-web frame.
    body: Vec<u8>,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            state: State::Head,
            raw: Vec::new(),
            body: Vec::new(),
        }
    }
}

fn line_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|pair| pair == b"\r\n")
}

impl Decoder {
    /// Feed the next bytes read from the connection.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Event>> {
        self.raw.extend_from_slice(bytes);
        let mut events = Vec::new();
        loop {
            let progressed = match self.state {
                State::Head => self.head(&mut events)?,
                State::Size => self.size()?,
                State::Data(left) => {
                    let take = left.min(self.raw.len());
                    self.body.extend(self.raw.drain(..take));
                    self.state = if take == left {
                        State::DataEnd
                    } else {
                        State::Data(left - take)
                    };
                    take > 0
                }
                State::DataEnd => match self.raw.get(..2) {
                    Some(b"\r\n") => {
                        self.raw.drain(..2);
                        self.state = State::Size;
                        true
                    }
                    Some(_) => return Err(Error::Wire),
                    None => false,
                },
                State::Length(left) => {
                    let take = left.min(self.raw.len());
                    self.body.extend(self.raw.drain(..take));
                    self.state = State::Length(left - take);
                    take > 0
                }
                State::Close => {
                    let any = !self.raw.is_empty();
                    self.body.append(&mut self.raw);
                    any
                }
                State::Done => {
                    self.raw.clear();
                    false
                }
            };
            self.frames(&mut events)?;
            if !progressed || matches!(self.state, State::Done) {
                return Ok(events);
            }
        }
    }

    /// Whether the call has ended, so a closed connection is expected.
    pub fn done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    fn head(&mut self, events: &mut Vec<Event>) -> Result<bool> {
        let mut headers = [httparse::EMPTY_HEADER; HEADERS];
        let mut response = httparse::Response::new(&mut headers);
        let used = match response.parse(&self.raw).map_err(|_| Error::Wire)? {
            httparse::Status::Complete(used) => used,
            httparse::Status::Partial if self.raw.len() > HEAD_LIMIT => return Err(Error::Size),
            httparse::Status::Partial => return Ok(false),
        };
        let code = response.code.unwrap_or_default();
        let mut trailers = Trailers::default();
        let mut chunked = false;
        let mut length = None;
        for header in response.headers.iter() {
            let value = String::from_utf8_lossy(header.value);
            trailers.header(header.name, &value);
            if header.name.eq_ignore_ascii_case("transfer-encoding") {
                chunked = value.to_ascii_lowercase().contains("chunked");
            } else if header.name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().ok();
            }
        }
        self.raw.drain(..used);
        // A refusal before any message is a reply of headers alone.
        if trailers.status.is_some() {
            self.state = State::Done;
            events.push(Event::End(trailers));
            return Ok(true);
        }
        if code != 200 {
            tracing::warn!(
                category = "boxd_exec",
                status = code,
                "boxd exec stream refused"
            );
            return Err(match code {
                401 => Error::Authentication,
                403 => Error::Forbidden,
                code => Status {
                    code: Code::Http(code),
                    message: None,
                }
                .into(),
            });
        }
        self.state = match (chunked, length) {
            (true, _) => State::Size,
            (false, Some(length)) => State::Length(length),
            (false, None) => State::Close,
        };
        Ok(true)
    }

    fn size(&mut self) -> Result<bool> {
        let Some(end) = line_end(&self.raw) else {
            return if self.raw.len() > SIZE_LINE_LIMIT {
                Err(Error::Wire)
            } else {
                Ok(false)
            };
        };
        let line = std::str::from_utf8(&self.raw[..end]).map_err(|_| Error::Wire)?;
        let digits = line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(digits, 16).map_err(|_| Error::Wire)?;
        if size > wire::FRAME_LIMIT + 5 {
            return Err(Error::Size);
        }
        self.raw.drain(..end + 2);
        // The last chunk; the call's status came in the trailer frame before it.
        self.state = if size == 0 {
            State::Done
        } else {
            State::Data(size)
        };
        Ok(true)
    }

    fn frames(&mut self, events: &mut Vec<Event>) -> Result<()> {
        while let Some((flag, payload, used)) = wire::take_frame(&self.body)? {
            if flag & TRAILER != 0 {
                events.push(Event::End(Trailers::parse(payload)));
                self.state = State::Done;
                self.body.clear();
                return Ok(());
            }
            let output = Output::decode(payload)?;
            if !output.data.is_empty() {
                events.push(if output.stderr {
                    Event::Stderr(output.data)
                } else {
                    Event::Stdout(output.data)
                });
            }
            if let Some(code) = output.exit_code {
                events.push(Event::Exit(code));
            }
            self.body.drain(..used);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
