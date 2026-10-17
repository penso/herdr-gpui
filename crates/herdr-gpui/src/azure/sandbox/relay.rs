//! This executable as a sandbox's standard streams. Cloud devices reach their
//! machines by running a command whose stdin and stdout are the remote shell's,
//! the way `ssh host COMMAND` does, and Herdr's bridge, probe, and installer
//! all append their remote command to it. A sandbox offers no SSH server, only
//! the data plane's exec stream: a WebSocket that carries JSON messages with
//! base64 stdin, stdout, and stderr and ends with the exit code. The app runs
//! itself with [`HELPER`] to translate between the two.
//!
//! The service runs `command` as one executable path, with no arguments, and
//! only delivers stdin to a process on a terminal. So the relay starts a bare
//! `/bin/sh` on a terminal and types one setup line: it makes the terminal raw
//! so bytes pass unchanged, prints [`output::START`], then reads the remote
//! command from stdin by length and runs it with stderr saved to a file. The
//! relay sends the remote command, and then the caller's input, only after
//! the start marker, so nothing reaches the shell before the terminal is raw.
//!
//! A terminal merges stdout and stderr, and no marker in that one stream can
//! tell where stdout ends without holding back bytes the bridge may be
//! waiting on. So everything after the start marker until the command exits
//! is its stdout, and once it exits the relay reads the saved stderr through
//! a second, short exec session.
//!
//! A raw terminal has no end of input, and closing the stream would end the
//! command's output too. So the caller's input travels in frames, each its
//! length on a line and then its bytes, and an empty frame is its end. A
//! reader on the remote side unpacks them into a FIFO the command reads, so
//! the command sees end of input the way it would over `ssh`.

use super::{
    super::{
        Error, Result,
        http::{Audience, Client},
    },
    api,
};
use base64::Engine as _;
use output::Output;
use secrecy::ExposeSecret;
use serde::Deserialize;
use std::{
    ffi::OsString,
    io::{self, Read, Write},
    net::TcpStream,
    process::ExitCode,
    sync::{
        Arc,
        mpsc::{self, Receiver, TryRecvError},
    },
    time::Duration,
};
use tungstenite::{
    Message, WebSocket, client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig,
};

/// The argument that makes this executable a sandbox relay.
pub(crate) const HELPER: &str = "__azure-sandbox-exec";
/// What `ssh` exits with when the connection, not the command, failed. The
/// bridge and installer read it the same way here.
const CONNECTION_FAILED: u8 = 255;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a read waits before the relay forwards pending input.
const POLL: Duration = Duration::from_millis(20);
const CHUNK: usize = 32 * 1024;
const MESSAGE_LIMIT: usize = 4 * 1024 * 1024;
const QUEUED_CHUNKS: usize = 64;

pub(super) mod output;

type Socket = WebSocket<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>;

/// Runs this executable as a relay when `args` asks for it, before anything
/// of the app starts.
pub(crate) fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    let (first, rest) = args.split_first()?;
    if first != HELPER {
        return None;
    }
    let [account, sandbox, remote @ ..] = rest else {
        eprintln!("Usage: herdr-gpui {HELPER} ACCOUNT SANDBOX [COMMAND...]");
        return Some(ExitCode::from(2));
    };
    Some(match run(account, sandbox, remote) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("herdr-gpui: {error}");
            ExitCode::from(CONNECTION_FAILED)
        }
    })
}

fn run(account: &OsString, sandbox: &OsString, remote: &[OsString]) -> Result<ExitCode> {
    let account = account.to_str().ok_or(Error::Encoding("account"))?;
    let sandbox = sandbox.to_str().ok_or(Error::Encoding("sandbox ID"))?;
    let config = crate::config::Config::load().map_err(|error| Error::Storage(Box::new(error)))?;
    let settings = config
        .azure
        .settings()
        .map_err(|error| Error::Storage(Box::new(error)))?
        .ok_or(Error::Account)?;
    let settings = settings.for_device(account)?;
    let client = Client::for_audience(&settings, Audience::Sandboxes)?;
    let url = api::exec_stream_url(&settings, sandbox)?;
    let mut socket = open(&url, client.token().expose_secret())?;
    socket
        .get_ref()
        .sock
        .set_read_timeout(Some(POLL))
        .map_err(Error::Process)?;
    let files = remote_files();
    let command = start(&mut socket, remote, &files)?;
    let code = relay(&mut socket, &command, input(), &mut io::stdout().lock())?;
    let _ = socket.close(None);
    let mut stderr = io::stderr().lock();
    // The command ran and its exit code stands; only its diagnostics are
    // missing, so say so rather than fail the command.
    let fetched = open(&url, client.token().expose_secret()).and_then(|mut socket| {
        socket
            .get_ref()
            .sock
            .set_read_timeout(Some(POLL))
            .map_err(Error::Process)?;
        fetch_errors(&mut socket, &files, &mut stderr)
    });
    if let Err(error) = fetched {
        let _ = writeln!(
            stderr,
            "herdr-gpui: could not read the sandbox command's stderr: {error}"
        );
    }
    Ok(code)
}

/// Where a remote command's saved stderr and its input FIFO go, as a path
/// prefix unique to this run.
fn remote_files() -> String {
    format!("/tmp/herdr-relay-{}", uuid::Uuid::new_v4().simple())
}

/// Start the remote shell and type its setup line, which keeps the
/// command's stderr and input FIFO at `files`. Returns the remote command,
/// which the shell reads once the terminal is raw.
pub(super) fn start<S: Read + Write>(
    socket: &mut WebSocket<S>,
    remote: &[OsString],
    files: &str,
) -> Result<Vec<u8>> {
    let command = remote_command(remote)?;
    socket
        .send(Message::text(start_message()))
        .map_err(Error::Stream)?;
    socket
        .send(Message::text(stdin_message(
            setup_line(command.len(), files).as_bytes(),
        )))
        .map_err(Error::Stream)?;
    Ok(command)
}

/// Write the stderr a finished command saved at `files` to `stderr`, through
/// a new exec session on `socket`, and delete it there. That session's own
/// stderr goes with it: nothing would read it.
pub(super) fn fetch_errors<S: Read + Write>(
    socket: &mut WebSocket<S>,
    files: &str,
    stderr: &mut impl Write,
) -> Result<()> {
    let own = remote_files();
    let remote = [OsString::from(format!(
        "cat {files}.err 2>/dev/null; rm -f {files}.err {own}.err"
    ))];
    let command = start(socket, &remote, &own)?;
    // No input: the empty frame goes at once.
    let (_, input) = mpsc::sync_channel(0);
    relay(socket, &command, input, stderr)?;
    let _ = socket.close(None);
    Ok(())
}

/// The remote command as `ssh` joins its arguments.
pub(super) fn remote_command(remote: &[OsString]) -> Result<Vec<u8>> {
    let words = remote
        .iter()
        .map(|word| word.to_str().ok_or(Error::Encoding("remote command")))
        .collect::<Result<Vec<_>>>()?;
    Ok(words.join(" ").into_bytes())
}

/// Typed while the terminal still echoes, so the start marker is spelled
/// with a `printf` escape that the echo shows literally. The shell the
/// service starts is interactive, which brings job control, job notices on
/// the terminal, and history expansion, so once the terminal is raw the rest
/// runs in a non-interactive `sh`, quoted whole so the interactive shell
/// expands none of it. `head -c` reads exactly the command, and then each
/// frame of the caller's input; see [`input_frame`]. The frame reader runs in
/// the background with the terminal as its explicit input, since a
/// background list's input is otherwise `/dev/null`, and is killed once the
/// command exits, as `ssh` stops forwarding input then. `files` is a path
/// prefix of plain characters, from [`remote_files`].
pub(super) fn setup_line(command_len: usize, files: &str) -> String {
    format!(
        "stty raw -echo -iexten || exit {CONNECTION_FAILED}; \
         exec /bin/sh -c '\
         e={files}.err; f={files}.in; \
         mkfifo \"$f\" || exit {CONNECTION_FAILED}; \
         printf \"\\001herdr-relay-start\\001\"; \
         s=$(head -c {command_len}); \
         exec 3<&0; \
         {{ while IFS= read -r n && [ \"$n\" -gt 0 ]; do head -c \"$n\"; done; }} <&3 >\"$f\" & r=$!; \
         eval \"$s\" <\"$f\" 2>\"$e\"; c=$?; \
         kill $r 2>/dev/null; rm -f \"$f\"; exit $c'\n"
    )
}

/// One frame of the caller's input as the setup line's reader unpacks it:
/// the length on a line, then the bytes. An empty `chunk` is end of input.
pub(super) fn input_frame(chunk: &[u8]) -> Vec<u8> {
    let mut frame = format!("{}\n", chunk.len()).into_bytes();
    frame.extend_from_slice(chunk);
    frame
}

fn open(url: &str, token: &str) -> Result<Socket> {
    let mut request = url.into_client_request().map_err(Error::Stream)?;
    let host = request
        .uri()
        .host()
        .ok_or(Error::Field("Azure sandbox endpoint"))?
        .to_owned();
    let mut authorization =
        HeaderValue::from_str(&format!("Bearer {token}")).map_err(Error::StreamHeader)?;
    authorization.set_sensitive(true);
    request.headers_mut().insert("Authorization", authorization);

    let address = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), 443))
        .map_err(Error::Process)?
        .next()
        .ok_or(Error::Field("Azure sandbox endpoint"))?;
    let tcp = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(Error::Process)?;
    tcp.set_read_timeout(Some(HANDSHAKE_TIMEOUT))
        .map_err(Error::Process)?;
    tcp.set_write_timeout(Some(HANDSHAKE_TIMEOUT))
        .map_err(Error::Process)?;
    tcp.set_nodelay(true).map_err(Error::Process)?;

    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(Error::Tls)?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(host)
        .map_err(|_| Error::Field("Azure sandbox endpoint"))?;
    let connection = rustls::ClientConnection::new(Arc::new(tls), name).map_err(Error::Tls)?;
    let stream = rustls::StreamOwned::new(connection, tcp);

    let config = WebSocketConfig::default()
        .max_message_size(Some(MESSAGE_LIMIT))
        .max_frame_size(Some(MESSAGE_LIMIT));
    let (socket, _) = tungstenite::client::client_with_config(request, stream, Some(config))
        .map_err(|error| match error {
            tungstenite::HandshakeError::Failure(error) => Error::Stream(error),
            tungstenite::HandshakeError::Interrupted(_) => Error::StreamClosed,
        })?;
    Ok(socket)
}

/// A terminal, because the service feeds stdin to nothing else; the setup
/// line makes it raw before any of the bridge's binary protocol passes.
fn start_message() -> String {
    serde_json::json!({
        "type": "start",
        "start": {
            "command": "/bin/sh",
            "environment": { "LANG": "C.UTF-8" },
            "tty": true,
            "stdin": true,
            "height": 24,
            "width": 80,
        },
    })
    .to_string()
}

pub(super) fn stdin_message(bytes: &[u8]) -> String {
    serde_json::json!({
        "type": "stdin",
        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
    })
    .to_string()
}

/// This process's stdin in chunks, read on its own thread because a blocking
/// read cannot wait alongside the socket. `None` is end of input.
fn input() -> Receiver<Option<Vec<u8>>> {
    let (send, receive) = mpsc::sync_channel(QUEUED_CHUNKS);
    std::thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut buffer = vec![0; CHUNK];
        loop {
            let chunk = match stdin.read(&mut buffer) {
                Ok(0) | Err(_) => None,
                Ok(read) => Some(buffer[..read].to_vec()),
            };
            let done = chunk.is_none();
            if send.send(chunk).is_err() || done {
                return;
            }
        }
    });
    receive
}

#[derive(Deserialize)]
struct Incoming {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    data: Option<String>,
    #[serde(default, rename = "exitCode")]
    exit_code: Option<i64>,
}

/// What one message from the stream asks the relay to do.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(u8),
    Failed(String),
    Ignore,
}

pub(super) fn event(text: &str) -> Result<Event> {
    let message: Incoming = serde_json::from_str(text).map_err(Error::Json)?;
    let decode = |data: Option<String>| {
        base64::engine::general_purpose::STANDARD
            .decode(data.unwrap_or_default())
            .map_err(|_| Error::StreamMessage)
    };
    Ok(match message.kind.as_str() {
        "stdout" => Event::Stdout(decode(message.data)?),
        "stderr" => Event::Stderr(decode(message.data)?),
        "exit_code" => Event::Exit(
            message
                .exit_code
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(CONNECTION_FAILED),
        ),
        "error" => Event::Failed(text.chars().take(512).collect()),
        _ => Event::Ignore,
    })
}

/// Forward `input` to the stream and its output to `stdout` and `stderr`
/// until the remote command exits. The socket's reads must time out, so that
/// input waiting to go out is not held behind a quiet remote.
pub(super) fn relay<S: Read + Write>(
    socket: &mut WebSocket<S>,
    command: &[u8],
    input: Receiver<Option<Vec<u8>>>,
    stdout: &mut impl Write,
) -> Result<ExitCode> {
    let mut output = Output::default();
    // Input that ends is passed on as an empty frame, which ends the remote
    // command's input but not the command: a probe runs with no input at
    // all and still has to hear its answer. The caller ends a bridge by
    // killing this process.
    let mut input_open = true;
    loop {
        while input_open && output.started() {
            let frame = match input.try_recv() {
                Ok(Some(chunk)) if chunk.is_empty() => continue,
                Ok(Some(chunk)) => input_frame(&chunk),
                Ok(None) | Err(TryRecvError::Disconnected) => {
                    input_open = false;
                    input_frame(&[])
                }
                Err(TryRecvError::Empty) => break,
            };
            socket
                .send(Message::text(stdin_message(&frame)))
                .map_err(Error::Stream)?;
        }
        let message = match socket.read() {
            Ok(message) => message,
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Err(Error::StreamClosed);
            }
            Err(error) => return Err(Error::Stream(error)),
        };
        let text = match message {
            Message::Text(text) => text,
            Message::Close(_) => return Err(Error::StreamClosed),
            _ => continue,
        };
        match event(text.as_str())? {
            // A terminal has one output stream; the service may still label
            // some of it stderr.
            Event::Stdout(bytes) | Event::Stderr(bytes) => {
                if output.push(&bytes, stdout)? && !command.is_empty() {
                    socket
                        .send(Message::text(stdin_message(command)))
                        .map_err(Error::Stream)?;
                }
            }
            // The service reports 0 when it could not start the shell at
            // all, so only a shell that reached the start marker has a
            // meaningful exit code.
            Event::Exit(_) if !output.started() => {
                let _ = socket.close(None);
                return Err(Error::Sandbox(output.diagnostic()));
            }
            Event::Exit(code) => return Ok(ExitCode::from(code)),
            Event::Failed(message) => return Err(Error::Sandbox(message)),
            Event::Ignore => {}
        }
    }
}
