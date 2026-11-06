//! This executable as a boxd machine's standard streams. Cloud devices reach
//! their machines by running a command whose stdin and stdout are the remote
//! shell's, the way `ssh host COMMAND` does, and Herdr's bridge, probe,
//! installer, and Teleport's scripts all append their remote command to it.
//! This app reaches a machine through the API's `Exec` stream instead, with
//! the API key it already holds, and runs itself with [`HELPER`] to translate
//! between the two (see [`stream`](super::stream) for the wire).
//!
//! Without a terminal the bytes pass unchanged, so the bridge's binary
//! protocol survives the trip, stderr stays apart from stdout, and the end
//! of this process's stdin reaches the command as end of file.

use super::{
    Error, Result, Settings, http, store,
    stream::{self, Decoder, Event},
};
use crate::cloud::Value;
use crate::cloud::socket::{self, pending};
use std::{
    ffi::OsString,
    io::{self, Read, Write},
    process::ExitCode,
    sync::mpsc::{Receiver, TryRecvError},
};

/// The argument that makes this executable a boxd relay.
pub const HELPER: &str = "__boxd-exec";
/// What `ssh` exits with when the connection, not the command, failed. The
/// bridge and installer read it the same way here.
const CONNECTION_FAILED: u8 = 255;
/// Input chunks taken before the relay reads again, so steady input, such as
/// a Git bundle, never keeps the command's output waiting.
const CHUNKS_PER_PASS: usize = 4;
/// Input waiting for the socket beyond which no more is taken from stdin.
const OUTGOING_LIMIT: usize = 256 * 1024;
const READ_CHUNK: usize = 64 * 1024;

/// Runs this executable as a relay when `args` asks for it, before anything
/// of the app starts.
pub fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    let (first, rest) = args.split_first()?;
    if first != HELPER {
        return None;
    }
    let [account, machine, remote @ ..] = rest else {
        eprintln!("Usage: sheprd {HELPER} ACCOUNT MACHINE COMMAND...");
        return Some(ExitCode::from(2));
    };
    if remote.is_empty() {
        eprintln!("Usage: sheprd {HELPER} ACCOUNT MACHINE COMMAND...");
        return Some(ExitCode::from(2));
    }
    Some(match run(account, machine, remote) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("sheprd: {error}");
            ExitCode::from(CONNECTION_FAILED)
        }
    })
}

fn run(account: &OsString, machine: &OsString, remote: &[OsString]) -> Result<u8> {
    let account = account.to_str().ok_or(Error::Encoding(Value::Account))?;
    let machine = machine
        .to_str()
        .ok_or(Error::Encoding(Value::BoxdMachine))?;
    if !super::settings::machine_reference(machine) {
        return Err(Error::Field(Value::BoxdMachine));
    }
    // Like `ssh`, the remote command is its arguments joined by spaces.
    let command = remote
        .iter()
        .map(|part| part.to_str().ok_or(Error::Encoding(Value::RemoteCommand)))
        .collect::<Result<Vec<_>>>()?
        .join(" ");
    let settings = super::connect::settings(account)?;
    let key = store::key(&settings)?;
    let mut connection = open(&settings, &key, machine, &command)?;
    relay(
        &mut connection,
        socket::input(io::stdin()),
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )
}

/// Connect to the API and send the request that starts `command` on
/// `machine`. Reads and writes then time out after [`socket::POLL`], so
/// input keeps flowing while the command is quiet.
pub(super) fn open(
    settings: &Settings,
    key: &secrecy::SecretString,
    machine: &str,
    command: &str,
) -> Result<socket::Stream> {
    let endpoint = settings.endpoint()?;
    let mut connection = socket::connect(endpoint.host, endpoint.port, endpoint.encrypted)?;
    connection
        .write_all(&stream::open(settings, key, machine, command))
        .and_then(|()| connection.flush())
        .map_err(Error::Stream)?;
    let tcp = connection.tcp();
    tcp.set_read_timeout(Some(socket::POLL))
        .and_then(|()| tcp.set_write_timeout(Some(socket::POLL)))
        .map_err(|error| Error::Socket(socket::Error::Connect(error)))?;
    Ok(connection)
}

/// Pass `input` to the command and its output to `stdout` and `stderr` until
/// the call ends, and return the command's exit code. The connection's reads
/// and writes must time out, so neither direction waits on the other.
pub(super) fn relay(
    connection: &mut (impl Read + Write),
    input: Receiver<Option<Vec<u8>>>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<u8> {
    let mut input = Some(input);
    let mut outgoing = Vec::new();
    let mut decoder = Decoder::default();
    let mut exit = 0;
    let mut buffer = vec![0; READ_CHUNK];
    loop {
        for _ in 0..CHUNKS_PER_PASS {
            let Some(receiver) = &input else {
                break;
            };
            if outgoing.len() >= OUTGOING_LIMIT {
                break;
            }
            match receiver.try_recv() {
                Ok(Some(chunk)) => stream::stdin(&mut outgoing, &chunk),
                Ok(None) | Err(TryRecvError::Disconnected) => {
                    outgoing.extend_from_slice(stream::END);
                    input = None;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        // A write that times out has sent part or none of it; the rest waits.
        if !outgoing.is_empty() && !decoder.done() {
            match connection.write(&outgoing) {
                Ok(0) => return Err(socket::Error::Closed.into()),
                Ok(sent) => drop(outgoing.drain(..sent)),
                Err(error) if pending(&error) => {}
                Err(error) => return Err(Error::Stream(error)),
            }
            match connection.flush() {
                Ok(()) => {}
                Err(error) if pending(&error) => {}
                Err(error) => return Err(Error::Stream(error)),
            }
        }
        let events = match connection.read(&mut buffer) {
            Ok(0) => return Err(socket::Error::Closed.into()),
            Ok(read) => decoder.push(&buffer[..read])?,
            Err(error) if pending(&error) => continue,
            Err(error) => return Err(Error::Stream(error)),
        };
        for event in events {
            match event {
                Event::Stdout(data) => {
                    stdout.write_all(&data).map_err(Error::Output)?;
                    stdout.flush().map_err(Error::Output)?;
                }
                Event::Stderr(data) => {
                    stderr.write_all(&data).map_err(Error::Output)?;
                    stderr.flush().map_err(Error::Output)?;
                }
                Event::Exit(code) => exit = code,
                Event::End(trailers) => {
                    return match trailers.status {
                        Some(0) => Ok(u8::try_from(exit).unwrap_or(CONNECTION_FAILED)),
                        _ => Err(http::failure("exec", &trailers)),
                    };
                }
            }
        }
        decoder.check()?;
    }
}
