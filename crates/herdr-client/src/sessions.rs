//! The named local sessions one machine's Herdr installation owns, and which of
//! them is running. A session socket outlives the daemon that created it, so a
//! session's state can only come from a connect attempt, never the file itself.
use crate::{
    Error, Result, Stream,
    catalog::validate_target,
    discovery::{config_dir, valid_session_name},
    session_socket,
};
#[cfg(unix)]
use crate::{
    limits::POLL,
    ssh::{CANDIDATES, SshChild, script_command},
};
use serde::Deserialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
#[cfg(unix)]
use std::{
    io::Read,
    os::fd::OwnedFd,
    process::Stdio,
    time::{Duration, Instant},
};

/// One scan reports at most this many sessions, like the endpoint catalog's
/// profile cap. More than this is refused rather than silently truncated.
const LIMIT: usize = 64;

/// The root session, which upstream keeps in the configuration directory itself
/// rather than in a subdirectory of `sessions/`.
const DEFAULT: &str = "default";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Running,
    Stopped,
}

impl SessionState {
    pub const fn running(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// A local session and the private endpoint this client would attach to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSession {
    pub name: String,
    pub state: SessionState,
    /// Derived by discovery, so callers never rebuild a session path themselves.
    pub socket: PathBuf,
}

/// A session one SSH host says it owns. Unlike a [`LocalSession`] this is the
/// remote CLI's own answer, not a socket this client probed: the host decided
/// `running`, and there is no remote socket to check it against.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RemoteSession {
    pub name: String,
    /// Absent from the host's JSON means the session is not running, never a guess.
    #[serde(default)]
    pub running: bool,
}

/// List the local sessions of a release or development installation, `default`
/// first and the rest by name. Performs filesystem and socket I/O in the calling
/// thread's time: call it from a worker, never a rendering thread.
pub fn list_local_sessions(development: bool) -> Result<Vec<LocalSession>> {
    list_with(&config_dir(development), |socket| {
        // A plain connect and drop is the same availability signal upstream's own
        // session list needs. The daemon logs a client only once it handshakes.
        Stream::connect(socket).is_ok()
    })
}

fn list_with(dir: &Path, running: impl Fn(&Path) -> bool) -> Result<Vec<LocalSession>> {
    let names = names(dir).map_err(|error| {
        Error::storage(crate::StorageOperation::Read, &session_root(dir), error)
    })?;
    if names.len() > LIMIT {
        return Err(Error::SessionLimit);
    }
    names
        .into_iter()
        .map(|name| {
            let socket = session_socket(dir, &name)?;
            let state = if running(&socket) {
                SessionState::Running
            } else {
                SessionState::Stopped
            };
            Ok(LocalSession {
                name,
                state,
                socket,
            })
        })
        .collect()
}

fn session_root(dir: &Path) -> PathBuf {
    dir.join("sessions")
}

/// `default` plus every session directory. A missing `sessions/` means this
/// installation has only ever used the root session, not that listing failed.
fn names(dir: &Path) -> io::Result<Vec<String>> {
    let mut names = vec![DEFAULT.to_owned()];
    let entries = match fs::read_dir(session_root(dir)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(names),
        Err(error) => return Err(error),
    };
    let mut named = Vec::new();
    for entry in entries {
        let entry = entry?;
        // file_type does not follow symlinks, so only real session directories
        // are listed and a link out of the configuration root is ignored.
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if let Some(name) = name.to_str()
            && valid_session_name(name)
        {
            named.push(name.to_owned());
        }
    }
    named.sort_unstable();
    names.append(&mut named);
    Ok(names)
}

/// The whole remote probe's budget: the figure the bridge's discovery already
/// allows, since a listing has to survive the same SSH handshake.
#[cfg(unix)]
const DEADLINE: Duration = Duration::from_secs(15);

/// A host listing `LIMIT` sessions with full socket paths stays well under this.
/// More than that is not a session list, and it is refused instead of buffered.
#[cfg(unix)]
const MAX_OUTPUT: usize = 64 * 1024;

/// The `herdr session list --json` envelope. Only the entries are read: the
/// host's `session_dir` and `socket_path` are its own filesystem, and the default
/// flag follows from a name this client already knows how to attach to.
#[cfg(unix)]
#[derive(Deserialize)]
struct SessionList {
    sessions: Vec<RemoteSession>,
}

/// List the sessions a saved SSH host owns by running `herdr session list --json`
/// there. The endpoint protocol has no session-list method and `herdr machine` has
/// no session subcommand, so the host's own CLI is the only route, and what it
/// reports is taken at its word. The target is validated before anything spawns.
///
/// This performs a blocking SSH round trip in the calling thread's time: call it
/// from a worker, never a rendering thread. A host with no discoverable Herdr, or
/// one reporting more sessions than a listing may hold, is an error rather than a
/// shorter list.
pub fn list_remote_sessions(target: &str) -> Result<Vec<RemoteSession>> {
    validate_target(target)?;
    #[cfg(unix)]
    {
        parse_session_list(&remote_stdout(target, &session_list_script())?)
    }
    // Handing a socket to a child as its stdout needs `OwnedFd`, which only Unix
    // gives, so this mirrors `ssh::connect`: reject the platform, not the target.
    #[cfg(not(unix))]
    {
        Err(Error::SshUnsupported)
    }
}

/// Ask the host for its own session list. Shares the bridge's discovery loop
/// because a non-interactive SSH `PATH` rarely names herdr, and `continue` past a
/// candidate that fails keeps one stale install from hiding a working one. A
/// candidate that answers ends the probe: without that `exit 0` every remaining
/// root lists its sessions too, and a run that worked exits 127.
#[cfg(unix)]
fn session_list_script() -> String {
    format!(
        r#"{CANDIDATES}
    if [ -n "$path" ] && [ -x "$path" ]; then
        "$path" session list --json || continue
        exit 0
    fi
done
exit 127"#
    )
}

/// One command's stdout over SSH: killed/reaped on every exit path by `SshChild`'s
/// `Drop`, so a host that never answers cannot leave an `ssh` process behind.
/// Stderr and stdin are discarded rather than inherited: remote diagnostics can
/// carry secrets and terminal controls, and a command that reads a terminal it was
/// not given would hang.
#[cfg(unix)]
fn remote_stdout(target: &str, script: &str) -> Result<Vec<u8>> {
    let (mut stream, child_stream) = Stream::pair()?;
    stream.set_read_timeout(Some(POLL))?;
    let mut command = script_command(target, script)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(child_stream)))
        .stderr(Stdio::null());
    let mut child = SshChild(command.spawn()?);
    read_listing(&mut stream, DEADLINE, || {
        matches!(child.0.try_wait(), Ok(Some(_)))
    })
}

/// A host's listing off `stream`, stopping as soon as one has arrived and holding
/// `timeout` as the bound for a host that never prints one. EOF is not the end of
/// one of these channels: a child that wrote three bytes and exited leaves the read
/// waiting, so `gone` says whether the writer has finished, which ends a short or
/// absent listing at once rather than at the deadline.
#[cfg(unix)]
fn read_listing(
    stream: &mut impl Read,
    timeout: Duration,
    gone: impl FnMut() -> bool,
) -> Result<Vec<u8>> {
    let started = Instant::now();
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    let mut gone = gone;
    loop {
        if started.elapsed() >= timeout {
            return Err(Error::SshTimeout);
        }
        match stream.read(&mut buffer) {
            // The child closed its end: the listing is whatever came before, which
            // is also how a host that printed nothing is reported as closed.
            Ok(0) => return Ok(output),
            Ok(read) => {
                output.extend_from_slice(&buffer[..read]);
                if output.len() > MAX_OUTPUT {
                    return Err(Error::SshOutputLimit);
                }
                if listed_sessions(&output) {
                    return Ok(output);
                }
            }
            Err(error) if idle(&error) && gone() => return Ok(output),
            Err(error) if idle(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

/// Whether an error means "nothing to read yet" rather than a broken channel.
#[cfg(unix)]
fn idle(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

/// Whether a host has already printed a complete session list, which is the point
/// past which reading more can only wait for an EOF that may never come.
#[cfg(unix)]
fn listed_sessions(output: &[u8]) -> bool {
    output
        .split(|byte| *byte == b'\n')
        .any(|line| serde_json::from_slice::<SessionList>(line).is_ok())
}

/// The bytes a host printed, as sessions. The last line that parses wins, because
/// SSH can put a banner or motd in front of the JSON, and a host that printed no
/// JSON at all is an error rather than a machine with no sessions. Names this
/// client could never attach to are dropped, and more than `LIMIT` sessions is
/// refused rather than truncated. Order is the local listing's: `default` first,
/// then by name.
#[cfg(unix)]
fn parse_session_list(output: &[u8]) -> Result<Vec<RemoteSession>> {
    // An empty response is the child closing stdout without printing anything:
    // SSH failed, or no candidate ran. It is not a host that has no sessions.
    if output.is_empty() {
        return Err(Error::SshClosed);
    }
    let mut listed = None;
    let mut invalid = None;
    for line in output.split(|byte| *byte == b'\n') {
        match serde_json::from_slice::<SessionList>(line) {
            Ok(list) => listed = Some(list),
            Err(error) => invalid = Some(error),
        }
    }
    let sessions = match (listed, invalid) {
        (Some(list), _) => list.sessions,
        // Report the last failure: the banner is noise, the JSON was the answer.
        (None, Some(error)) => return Err(Error::Json(error)),
        // Unreachable after the empty check: a non-empty output has a line to fail.
        (None, None) => return Err(Error::SshClosed),
    };
    if sessions.len() > LIMIT {
        return Err(Error::SessionLimit);
    }
    let mut sessions: Vec<RemoteSession> = sessions
        .into_iter()
        .filter(|session| valid_session_name(&session.name))
        .collect();
    sessions.sort_by(|a, b| {
        (a.name != DEFAULT, a.name.as_str()).cmp(&(b.name != DEFAULT, b.name.as_str()))
    });
    Ok(sessions)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::{
        cell::RefCell,
        env,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    /// A private configuration directory; its build is idempotent so one test can
    /// add entries to it after the first assertion.
    fn fixture() -> PathBuf {
        let root = env::temp_dir().join(format!(
            "herdr-sessions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("sessions")).unwrap();
        fs::write(root.join("config.toml"), "version = 1").unwrap();
        root
    }

    /// A root for the tests that bind a socket. The temporary directory on macOS
    /// is already half of `sun_path`, so this one is built short and under `/tmp`:
    /// a session socket is `sessions/<name>/herdr-client.sock` longer than it.
    #[cfg(unix)]
    fn socket_fixture() -> PathBuf {
        let root = Path::new("/tmp").join(format!(
            "hd-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // A run that panicked leaves its socket behind, and bind would refuse it.
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sessions")).unwrap();
        root
    }

    fn names_of(sessions: &[LocalSession]) -> Vec<&str> {
        sessions.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn only_real_directories_are_listed_in_order() {
        let root = fixture();
        for name in ["work", "gami9", "projects"] {
            fs::create_dir(root.join("sessions").join(name)).unwrap();
        }
        // A session directory without a socket is still a session: it is stopped.
        fs::write(root.join("sessions/notes.txt"), "not a session").unwrap();
        fs::create_dir(root.join("sessions/bad name")).unwrap();
        // Dots are legal in a session name (`work.1`), so a dot directory counts.
        fs::create_dir(root.join("sessions/.hidden")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("work"), root.join("sessions/linked")).unwrap();

        let probed = RefCell::new(Vec::new());
        let sessions = list_with(&root, |socket| {
            probed.borrow_mut().push(socket.to_owned());
            socket.ends_with("sessions/work/herdr-client.sock")
        })
        .unwrap();
        assert_eq!(
            names_of(&sessions),
            ["default", ".hidden", "gami9", "projects", "work"]
        );
        assert_eq!(
            probed.into_inner(),
            ["default", ".hidden", "gami9", "projects", "work"]
                .map(|name| session_socket(&root, name).unwrap())
        );
        assert_eq!(sessions[0].state, SessionState::Stopped);
        assert_eq!(sessions[4].state, SessionState::Running);
        assert!(SessionState::Running.running() && !SessionState::Stopped.running());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_single_session_installation_lists_only_the_root() {
        let root = env::temp_dir().join(format!(
            "herdr-sessions-bare-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let sessions = list_with(&root, |_| false).unwrap();
        assert_eq!(names_of(&sessions), ["default"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn excessive_session_counts_are_refused_rather_than_truncated() {
        let root = fixture();
        // `default` is one of the sessions, so the cap counts it too.
        for index in 1..LIMIT {
            fs::create_dir(root.join("sessions").join(format!("session{index}"))).unwrap();
        }
        assert_eq!(list_with(&root, |_| false).unwrap().len(), LIMIT);
        fs::create_dir(root.join("sessions").join("one-too-many")).unwrap();
        assert!(matches!(
            list_with(&root, |_| false),
            Err(Error::SessionLimit)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unreadable_session_roots_report_the_path_they_failed_on() {
        let root = fixture();
        fs::remove_dir_all(root.join("sessions")).unwrap();
        fs::write(root.join("sessions"), "not a directory").unwrap();
        let error = list_with(&root, |_| false).unwrap_err();
        assert!(matches!(
            error,
            Error::Storage {
                operation: crate::StorageOperation::Read,
                ref path,
                ..
            } if path == &root.join("sessions")
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn default_belongs_to_the_configuration_root() {
        let root = fixture();
        let sessions = list_with(&root, |_| false).unwrap();
        assert_eq!(sessions[0].socket, root.join("herdr-client.sock"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn state_follows_a_live_listener_not_the_socket_file() {
        let root = socket_fixture();
        let work = root.join("sessions/work");
        fs::create_dir(&work).unwrap();
        let path = work.join("herdr-client.sock");
        let listener = crate::transport::Listener::bind(&path).unwrap();
        assert!(running(&root, "work"));
        // The socket file survives the daemon, exactly as an installed one does.
        // A listener the kernel has only just closed still answers a connect for
        // an instant, so the state a surviving file implies is asserted against a
        // path no listener ever owned rather than raced against that teardown.
        drop(listener);
        assert!(path.exists());
        let stale = root.join("sessions/leftover");
        fs::create_dir(&stale).unwrap();
        fs::write(stale.join("herdr-client.sock"), "").unwrap();
        assert!(!running(&root, "leftover"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    fn running(root: &Path, name: &str) -> bool {
        list_with(root, |socket| Stream::connect(socket).is_ok())
            .unwrap()
            .into_iter()
            .find(|session| session.name == name)
            .unwrap()
            .state
            .running()
    }

    /// The verified output of a real `herdr session list --json`, verbatim.
    #[cfg(unix)]
    const LISTING: &str = r#"{"sessions":[{"default":true,"name":"default","running":true,"session_dir":"/home/ed/.config/herdr","socket_path":"/home/ed/.config/herdr/herdr.sock"},{"default":false,"name":"gami","running":true,"session_dir":"/home/ed/.config/herdr/sessions/gami","socket_path":"/home/ed/.config/herdr/sessions/gami/herdr.sock"}]}"#;

    #[cfg(unix)]
    fn listed(count: usize) -> String {
        let entries: Vec<String> = (0..count)
            .map(|index| format!(r#"{{"name":"session{index}","running":true}}"#))
            .collect();
        format!(r#"{{"sessions":[{}]}}"#, entries.join(","))
    }

    #[cfg(unix)]
    #[test]
    fn a_host_listing_is_parsed_in_the_local_order() {
        assert_eq!(
            parse_session_list(LISTING.as_bytes()).unwrap(),
            [
                RemoteSession {
                    name: "default".into(),
                    running: true,
                },
                RemoteSession {
                    name: "gami".into(),
                    running: true,
                },
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_host_that_omits_the_running_flag_lists_a_stopped_session() {
        assert_eq!(
            parse_session_list(r#"{"sessions":[{"name":"default"}]}"#.as_bytes()).unwrap(),
            [RemoteSession {
                name: "default".into(),
                running: false,
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn names_this_client_cannot_attach_to_are_dropped() {
        assert_eq!(
            parse_session_list(
                r#"{"sessions":[{"name":"bad/name","running":true},{"name":".."},{"name":"work","running":true}]}"#
                    .as_bytes()
            )
            .unwrap(),
            [RemoteSession {
                name: "work".into(),
                running: true,
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_banner_before_the_listing_is_tolerated_and_the_last_line_wins() {
        // SSH can print a banner or motd ahead of the JSON.
        let bannered = format!("Welcome to Ubuntu\nLast login: Thu 1 Jan\n{LISTING}\n");
        assert_eq!(parse_session_list(bannered.as_bytes()).unwrap().len(), 2);
        // Two listings on one connection: the later one is the host's answer.
        let twice = format!(
            r#"{{"sessions":[{{"name":"stale","running":true}}]}}
{LISTING}"#
        );
        assert_eq!(parse_session_list(twice.as_bytes()).unwrap().len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn output_that_is_not_a_listing_is_a_failure_not_an_empty_listing() {
        assert!(matches!(
            parse_session_list(b"not json\n"),
            Err(Error::Json(_))
        ));
        assert!(matches!(
            parse_session_list(r#"{"sessions":"unexpected"}"#.as_bytes()),
            Err(Error::Json(_))
        ));
    }

    #[cfg(unix)]
    /// A channel that prints what it was given and then goes quiet without ever
    /// closing, the way a host that answered leaves one.
    struct Answered(std::collections::VecDeque<u8>);

    #[cfg(unix)]
    impl Read for Answered {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            let take = buffer.len().min(self.0.len());
            let chunk: Vec<u8> = (0..take).filter_map(|_| self.0.pop_front()).collect();
            buffer[..take].copy_from_slice(&chunk);
            Ok(take)
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_host_that_answered_is_read_without_waiting_for_the_channel_to_close() {
        let listing = br#"{"sessions":[{"name":"default","running":true}]}"#;
        let mut stream = Answered(listing.iter().copied().collect());
        let started = Instant::now();
        let output = match read_listing(&mut stream, Duration::from_secs(30), || false) {
            Ok(output) => output,
            Err(error) => panic!("this host answered, so it must not fail: {error}"),
        };
        assert_eq!(output, listing);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the read waited for an EOF this host never sends"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_child_that_finished_ends_the_read_instead_of_the_deadline() {
        // A host with no usable Herdr prints nothing and exits, which is a failure
        // worth reporting at once rather than a fifteen-second timeout.
        let mut silent = Answered(std::collections::VecDeque::new());
        let started = Instant::now();
        match read_listing(&mut silent, Duration::from_secs(30), || true) {
            Ok(output) => assert!(output.is_empty()),
            Err(error) => panic!("this child is gone, so it must not be waited on: {error}"),
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        // Whatever it did print is kept, so the parser can still report on it.
        let mut banner = Answered(b"host: no herdr here\n".iter().copied().collect());
        match read_listing(&mut banner, Duration::from_secs(30), || true) {
            Ok(output) => assert_eq!(output, b"host: no herdr here\n"),
            Err(error) => panic!("this child is gone, so it must not be waited on: {error}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_channel_that_never_prints_a_listing_still_times_out() {
        let mut stream = Answered(std::collections::VecDeque::new());
        assert!(matches!(
            read_listing(&mut stream, Duration::from_millis(20), || false),
            Err(Error::SshTimeout)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_complete_listing_ends_the_read_before_any_eof() {
        // The command that prints a listing leaves a process holding the SSH
        // session open, so an answer must not be waited for until EOF: doing that
        // reported a host answering in a second as a fifteen-second timeout.
        assert!(listed_sessions(br#"{"sessions":[]}"#));
        assert!(listed_sessions(
            b"motd: welcome\n{\"sessions\":[{\"name\":\"default\",\"running\":true}]}\n"
        ));
        // A line that is still arriving is not an answer, so the deadline stands.
        assert!(!listed_sessions(b"{\"sessions\":[{\"name\":\"def"));
        assert!(!listed_sessions(b"motd: welcome\n"));
        assert!(!listed_sessions(b""));
    }

    #[cfg(unix)]
    #[test]
    fn a_host_that_prints_nothing_is_a_closed_bridge() {
        assert!(matches!(parse_session_list(b""), Err(Error::SshClosed)));
    }

    #[cfg(unix)]
    #[test]
    fn an_excessive_remote_listing_is_refused_rather_than_truncated() {
        assert_eq!(
            parse_session_list(listed(LIMIT).as_bytes()).unwrap().len(),
            LIMIT
        );
        assert!(matches!(
            parse_session_list(listed(LIMIT + 1).as_bytes()),
            Err(Error::SessionLimit)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn the_remote_listing_walks_the_shared_candidate_roots() {
        // Asserted on the script and on the SSH child's arguments, never by faking
        // an `ssh` binary on PATH.
        let script = session_list_script();
        assert!(
            script.contains(r#""$path" session list --json"#),
            "{script}"
        );
        assert!(script.contains("command -v herdr"), "{script}");
        // The candidate that answers ends the probe, with a zero status.
        assert!(script.contains("exit 0"), "{script}");
        assert!(script.starts_with(CANDIDATES), "{script}");
        let command = script_command("host", &script).unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        let wrapped = format!("/bin/sh -c {}", crate::ssh::quote(&script));
        assert_eq!(args.last(), Some(&wrapped.as_str()));
        assert!(args.contains(&"BatchMode=yes"));
        // A malformed target is rejected before anything is built or spawned.
        assert!(matches!(
            list_remote_sessions("-oProxyCommand=bad"),
            Err(Error::InvalidSshTarget)
        ));
    }
}
