//! The step that lists this computer's TCP listeners, and the text it prints.
//!
//! Linux prefers `ss`, which is quicker; macOS has only `lsof`. Both name a
//! socket's process only when it is this user's, so another user's servers
//! never come back with a pid and are dropped here. The script names the
//! tool it ran on its first line, so the parser never guesses a format.

use crate::listening_ports::{Bind, listener};

/// Socket lines read from one answer; the rest are ignored.
pub(super) const MAX_SOCKETS: usize = 1024;

/// Prints `ss`, `lsof`, or `none`, then that tool's own listing. `lsof`
/// exits 1 when nothing listens, so the script ends with `true` for the
/// answer to count; the tools' own errors are dropped.
// i18n: data
pub(super) const SCRIPT: &str = r#"PATH="$PATH:/usr/sbin:/sbin"
if command -v ss >/dev/null 2>&1; then
    printf 'ss\n'
    ss -ltnp 2>/dev/null
elif command -v lsof >/dev/null 2>&1; then
    printf 'lsof\n'
    lsof -nP -iTCP -sTCP:LISTEN -Fpcn 2>/dev/null
else
    printf 'none\n'
fi
true
"#;

/// One listening socket: the process holding it, and where it listens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Socket {
    pub pid: u32,
    pub number: u16,
    pub bind: Bind,
    /// The tool's name for the process, bounded display text.
    pub process: String,
}

/// What [`SCRIPT`] printed, as sockets, or `None` when the computer has
/// neither tool. Each process's IPv4 and IPv6 sockets on one number are one
/// row, kept at the wider of the two binds; the result is sorted by port,
/// then pid.
pub(crate) fn parse(text: &str) -> Option<Vec<Socket>> {
    let mut lines = text.lines();
    let mut sockets = match lines.next().map(str::trim) {
        Some("ss") => ss(lines),
        Some("lsof") => lsof(lines),
        _ => return None,
    };
    sockets.sort_by(|a, b| {
        (a.number, a.pid)
            .cmp(&(b.number, b.pid))
            .then(b.bind.reach().cmp(&a.bind.reach()))
    });
    sockets.dedup_by(|later, kept| later.number == kept.number && later.pid == kept.pid);
    Some(sockets)
}

fn socket(pid: &str, address: &str, process: &str) -> Option<Socket> {
    let pid = pid.parse().ok().filter(|pid| *pid > 0)?;
    let port = listener(address, Some(process))?;
    Some(Socket {
        pid,
        number: port.number,
        bind: port.bind,
        process: port.process,
    })
}

/// `ss -ltnp`: `LISTEN 0 4096 127.0.0.1:3000 0.0.0.0:* users:(("node",pid=4242,fd=20))`,
/// one line per socket, naming each process that holds it. The header and
/// sockets without a process (another user's) are skipped.
fn ss<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<Socket> {
    let mut sockets = Vec::new();
    for line in lines {
        let mut fields = line.split_whitespace();
        let (Some("LISTEN"), Some(address)) = (fields.next(), fields.nth(2)) else {
            continue;
        };
        let mut rest = line;
        while let Some(start) = rest.find("(\"") {
            rest = &rest[start + 2..];
            let Some((name, after)) = rest.split_once("\",pid=") else {
                break;
            };
            let digits = after
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after.len());
            if sockets.len() == MAX_SOCKETS {
                return sockets;
            }
            if let Some(socket) = socket(&after[..digits], address, name) {
                sockets.push(socket);
            }
            rest = &after[digits..];
        }
    }
    sockets
}

/// `lsof -F pcn`: a `p` line starts each process, its `c` line names it,
/// and each `n` line after it is one of its sockets (`f` lines between are
/// descriptors, ignored).
fn lsof<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<Socket> {
    let mut sockets = Vec::new();
    let (mut pid, mut name) = ("", "");
    for line in lines {
        let Some(tag) = line.chars().next() else {
            continue;
        };
        let value = &line[tag.len_utf8()..];
        match tag {
            'p' => (pid, name) = (value, ""),
            'c' => name = value,
            'n' if sockets.len() == MAX_SOCKETS => break,
            'n' => sockets.extend(socket(pid, value, name)),
            _ => {}
        }
    }
    sockets
}
