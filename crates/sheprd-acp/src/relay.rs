//! `herdr-acp connect ID`: carries one client connection to the chat whose
//! endpoint id is `ID` over this process's standard input and output. Herdr
//! UI runs it over SSH to attach to a chat on another machine; the program
//! there sees an ordinary client on its own socket, under its own user's
//! private directory, so nothing about trust changes. Bytes pass through
//! unread: the attach protocol runs end to end.

use std::{
    io::{self, Read, Write},
    thread,
    time::Duration,
};

use sheprd_chat_protocol::EndpointId;
use sheprd_client::private_endpoint::Connection;

use crate::{Error, Result};

/// How long connecting waits for a free pipe instance on Windows.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const CHUNK: usize = 64 * 1024;

pub(crate) fn run(id: &EndpointId) -> Result<()> {
    let address = sheprd_chat_protocol::endpoint_address(id).ok_or(Error::NoEndpointDir)?;
    let (reader, writer, closer) = Connection::connect(&address, CONNECT_TIMEOUT)?.split()?;
    let upstream_closer = closer.try_clone()?;
    // Client to program; the client closing its end ends the connection.
    thread::Builder::new()
        .name("herdr-acp-relay".into())
        .spawn(move || {
            let _ = pump(io::stdin().lock(), writer);
            upstream_closer.close();
        })?;
    // Program to client, flushed per read: frames have no newline for a
    // line-buffered stdout to wait for.
    let result = pump(reader, io::stdout().lock());
    closer.close();
    Ok(result?)
}

/// Copies until either side ends, flushing after every read.
fn pump(mut from: impl Read, mut to: impl Write) -> io::Result<()> {
    let mut buffer = vec![0; CHUNK];
    loop {
        let read = match from.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        to.write_all(&buffer[..read])?;
        to.flush()?;
    }
}

#[cfg(test)]
mod tests;
