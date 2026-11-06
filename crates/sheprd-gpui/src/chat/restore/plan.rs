//! What the restore pass decides, apart from files, sockets, and the daemon,
//! so every fence is tested deterministically: which records qualify, when
//! a shell is idle, when a pass is due, when a watched chat has ended, and
//! the line typed into the shell.

use crate::teleport::ProcessInfo;
use sheprd_chat_protocol::{EndpointId, TOKEN, restore};
use sheprd_client::protocol::ClientShellSnapshot;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long the restore pass waits between its runs for one scope.
pub(in crate::chat) const PASS_INTERVAL: Duration = Duration::from_secs(5);
/// How many times the restore pass runs for one scope.
pub(in crate::chat) const PASSES: u8 = 6;
/// How long a chat seen live may stay out of the snapshot, under the same
/// boot and connection, before it counts as ended there and its record is
/// dropped. Long enough that a window still connected while the machine
/// shuts down (when chats may die before their daemon) is gone first.
pub(in crate::chat) const ENDED_AFTER: Duration = Duration::from_secs(30);
/// How long a typed line has to start a chat that shows in its pane before
/// the claim is given back. A chat still starting keeps its shell busy, so
/// the record is not typed again while it does.
pub(in crate::chat) const CONFIRM_WITHIN: Duration = Duration::from_secs(60);
/// How old a claim may be when the window reads it: an older one rests on
/// an idle check that no longer says much, so it is given back instead.
pub(in crate::chat) const CLAIM_FRESH: Duration = Duration::from_secs(10);
/// Live chats one watch job stamps; the rest wait for the next.
pub(in crate::chat) const MAX_STAMPS: usize = super::super::MAX_CHATS;
/// Panes the pass asks Herdr about in one run; the rest wait for the next.
pub(in crate::chat) const MAX_PROBES: usize = super::super::MAX_CHATS;

/// The connection and daemon boot a pass belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::chat) struct Scope {
    pub(in crate::chat) boot: String,
    pub(in crate::chat) connection: u64,
    pub(in crate::chat) selection: u64,
}

/// A pane as the snapshot shows it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::chat) struct Pane {
    /// Its folder as the snapshot reports it, for when Herdr cannot say
    /// where its shell is.
    pub(in crate::chat) cwd: Option<String>,
    pub(in crate::chat) id: String,
    /// Whether an agent or a chat is reported in it.
    pub(in crate::chat) occupied: bool,
}

/// The panes of `snapshot`, each marked occupied when the daemon reports an
/// agent or a chat token in it.
pub(in crate::chat) fn panes(snapshot: &ClientShellSnapshot) -> Vec<Pane> {
    let occupied: HashSet<&str> = snapshot
        .agents
        .iter()
        .filter(|agent| agent.agent.is_some() || agent.tokens.iter().any(|(name, _)| name == TOKEN))
        .map(|agent| agent.pane_id.as_str())
        .collect();
    snapshot
        .panes
        .iter()
        .map(|pane| Pane {
            cwd: pane.foreground_cwd.clone().or_else(|| pane.cwd.clone()),
            id: pane.pane_id.clone(),
            occupied: occupied.contains(pane.pane_id.as_str()),
        })
        .collect()
}

/// Every chat live in `snapshot`, with the pane it runs in. Not capped:
/// whether a watched chat ended is judged against all of them, and a chat
/// left out would look ended.
pub(in crate::chat) fn live_chats(snapshot: &ClientShellSnapshot) -> Vec<(EndpointId, String)> {
    snapshot
        .agents
        .iter()
        .filter_map(|agent| {
            Some((
                super::super::discovery::endpoint(&agent.tokens)?,
                agent.pane_id.clone(),
            ))
        })
        .collect()
}

/// The live chats not yet stamped this scope, at most
/// [`MAX_STAMPS`] of them, for one job; the rest wait for the next.
pub(in crate::chat) fn to_stamp(
    live: Vec<(EndpointId, String)>,
    stamped: &HashSet<EndpointId>,
) -> Vec<(EndpointId, String)> {
    live.into_iter()
        .filter(|(endpoint, _)| !stamped.contains(endpoint))
        .take(MAX_STAMPS)
        .collect()
}

/// Whether `pane` is in `panes` and free to type into.
pub(in crate::chat) fn free(panes: &[Pane], pane: &str) -> bool {
    panes
        .iter()
        .any(|known| known.id == pane && !known.occupied)
}

/// The records worth checking further for `boot`, in the order given: of
/// this daemon (`owns` their socket), seen live under another boot, naming
/// a free pane that no other record of this daemon names. Another Herdr
/// session's panes have ids of their own, so its records never count.
pub(in crate::chat) fn plan(
    records: Vec<(restore::Record, Option<String>)>,
    panes: &[Pane],
    boot: &str,
    owns: impl Fn(&Path) -> bool,
) -> Vec<restore::Record> {
    let ours: Vec<(restore::Record, Option<String>)> = records
        .into_iter()
        .filter(|(record, _)| owns(&record.socket))
        .collect();
    let mut named: HashMap<&str, usize> = HashMap::new();
    for (record, _) in &ours {
        *named.entry(record.pane.as_str()).or_default() += 1;
    }
    // Two chats once in one pane: which is the latest cannot be told, so
    // neither is continued.
    let single: HashSet<String> = named
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .map(|(pane, _)| pane.to_owned())
        .collect();
    ours.into_iter()
        .filter(|(_, seen)| seen.as_deref().is_some_and(|seen| seen != boot))
        .map(|(record, _)| record)
        .filter(|record| single.contains(&record.pane))
        .filter(|record| free(panes, &record.pane))
        .collect()
}

/// The records of this daemon (`owns` their socket) that a chat now live
/// in the same pane replaced: a pane runs one chat, and the live one is the
/// one to continue. Without this, a chat started in a pane whose old chat
/// was never removed would leave two records naming it, and neither would
/// be continued.
pub(in crate::chat) fn superseded<'a>(
    records: &'a [(restore::Record, Option<String>)],
    live: &[(EndpointId, String)],
    owns: impl Fn(&Path) -> bool,
) -> impl Iterator<Item = &'a EndpointId> {
    records
        .iter()
        .map(|(record, _)| record)
        .filter(move |record| {
            owns(&record.socket)
                && live
                    .iter()
                    .any(|(endpoint, pane)| *pane == record.pane && *endpoint != record.endpoint)
        })
        .map(|record| &record.endpoint)
}

/// The records of this daemon (`owns` their socket) that no restore can
/// use any more: seen live under an earlier boot, and their pane is gone.
/// A record never seen live may belong to a chat whose pane is newer than
/// the snapshot, so [`restore::prune`] ages those out instead.
pub(in crate::chat) fn orphans<'a>(
    records: &'a [(restore::Record, Option<String>)],
    panes: &[Pane],
    boot: &str,
    owns: impl Fn(&Path) -> bool,
) -> impl Iterator<Item = &'a EndpointId> {
    records
        .iter()
        .filter(move |(record, seen)| {
            seen.as_deref().is_some_and(|seen| seen != boot)
                && owns(&record.socket)
                && !panes.iter().any(|pane| pane.id == record.pane)
        })
        .map(|(record, _)| &record.endpoint)
}

/// Whether Herdr's `info` shows the pane's shell itself in the foreground,
/// at its prompt, in `cwd`. An answer that does not name both the shell and
/// the foreground group is not idle: unknown is never safe to type into.
/// The shell's folder is its own as Herdr lists it, else `fallback` (the
/// snapshot's), each made canonical by `canonical`.
pub(in crate::chat) fn idle(
    info: &ProcessInfo,
    fallback: Option<&str>,
    cwd: &Path,
    canonical: impl Fn(&str) -> Option<PathBuf>,
) -> bool {
    let (Some(shell), Some(group)) = (info.shell_pid, info.foreground_process_group_id) else {
        return false;
    };
    if shell != group {
        return false;
    }
    info.foreground_processes
        .iter()
        .find(|process| process.pid == shell)
        .and_then(|process| process.cwd.as_deref())
        .or(fallback)
        .and_then(canonical)
        .is_some_and(|shell_cwd| shell_cwd == cwd)
}

/// The run of the restore pass due for `scope` at `now`, counting from 1,
/// given the scope and count it last ran with and when it may run again.
/// None once [`PASSES`] runs are done, or before the next is due.
pub(in crate::chat) fn pass_due(
    ran: Option<&(Scope, u8)>,
    next: Option<Instant>,
    scope: &Scope,
    now: Instant,
) -> Option<u8> {
    match ran {
        Some((ran, count)) if ran == scope => {
            (*count < PASSES && next.is_none_or(|next| now >= next)).then_some(count + 1)
        }
        // A new boot or connection starts over at once.
        _ => Some(1),
    }
}

/// Chats seen live under one scope, and since when each has been missing.
#[derive(Debug, Default)]
pub(in crate::chat) struct Watched {
    chats: HashMap<EndpointId, Option<Instant>>,
    scope: Option<Scope>,
}

impl Watched {
    /// Notes the chats `live` in `scope`'s snapshot at `now`, and returns
    /// those missing from it for [`ENDED_AFTER`]: they ended while their
    /// daemon kept running, so nothing should continue them. A new scope
    /// forgets what the last one watched rather than ending it, as a restart
    /// or reconnect is no sign that a chat ended.
    pub(in crate::chat) fn observe(
        &mut self,
        scope: &Scope,
        live: &[EndpointId],
        now: Instant,
    ) -> Vec<EndpointId> {
        if self.scope.as_ref() != Some(scope) {
            self.chats.clear();
            self.scope = Some(scope.clone());
        }
        for endpoint in live {
            self.chats.insert(endpoint.clone(), None);
        }
        let mut ended = Vec::new();
        self.chats.retain(|endpoint, missing| {
            if live.contains(endpoint) {
                return true;
            }
            let since = *missing.get_or_insert(now);
            if now.saturating_duration_since(since) < ENDED_AFTER {
                return true;
            }
            ended.push(endpoint.clone());
            false
        });
        ended
    }
}

/// A record whose line was typed, held claimed until its chat shows.
#[derive(Clone, Debug)]
pub(in crate::chat) struct Typed {
    pub(in crate::chat) endpoint: EndpointId,
    pub(in crate::chat) pane: String,
    pub(in crate::chat) scope: Scope,
    pub(in crate::chat) since: Instant,
}

/// What became of the typed lines a window waits on.
#[derive(Debug, Default, Eq, PartialEq)]
pub(in crate::chat) struct Settled {
    /// A chat shows in their pane: their records are done with.
    pub(in crate::chat) confirmed: Vec<EndpointId>,
    /// Their line may never have arrived (the boot or connection changed,
    /// or chats were turned off), or no chat showed in time: their records
    /// are given back, and only an idle shell is ever typed into again.
    pub(in crate::chat) returned: Vec<EndpointId>,
}

/// Settles each of `typed` that can be: confirmed when `current` is its
/// scope and its pane holds a chat (`chat_panes`), returned when `current`
/// is another scope or none, or [`CONFIRM_WITHIN`] has passed. The rest
/// stay in `typed`.
pub(in crate::chat) fn settle(
    typed: &mut Vec<Typed>,
    current: Option<&Scope>,
    chat_panes: &HashSet<&str>,
    now: Instant,
) -> Settled {
    let mut settled = Settled::default();
    typed.retain(|line| {
        if current != Some(&line.scope) {
            settled.returned.push(line.endpoint.clone());
        } else if chat_panes.contains(line.pane.as_str()) {
            settled.confirmed.push(line.endpoint.clone());
        } else if now.saturating_duration_since(line.since) >= CONFIRM_WITHIN {
            settled.returned.push(line.endpoint.clone());
        } else {
            return true;
        }
        false
    });
    settled
}

/// The line typed into a restored shell to continue `record` with
/// `program`. `sh` runs it, so it reads the same from bash, zsh, fish, or
/// nushell; each argument rides as its own single-quoted word, and a value
/// holding a quote or a control character makes no line at all. A leading
/// space keeps it out of the history of shells that honor that.
pub(in crate::chat) fn line(program: &Path, record: &restore::Record) -> Option<String> {
    let program = program.to_str()?;
    let mut words = vec![program, "--resume", record.session.as_str()];
    if record.adapter {
        words.push("--acp");
    }
    words.push(record.agent.name());
    if !restore::session_id_is_plain(&record.session) {
        return None;
    }
    quoted_line(&words)
}

/// The line that runs `words`, the program first, through `sh`, each as its
/// own single-quoted word; `None` when one holds a quote or a control
/// character. See [`line`].
pub(in crate::chat) fn quoted_line(words: &[&str]) -> Option<String> {
    if words
        .iter()
        .any(|word| word.contains('\'') || word.chars().any(char::is_control))
    {
        return None;
    }
    let quoted: Vec<String> = words.iter().map(|word| format!("'{word}'")).collect();
    Some(format!(" sh -c 'exec \"$0\" \"$@\"' {}", quoted.join(" ")))
}

/// How the pass learns what it cannot see in a record or a snapshot.
pub(in crate::chat) struct Probes<O, L, I> {
    /// Whether a record's Herdr API socket is this daemon's.
    pub(in crate::chat) owns: O,
    /// Whether a program still answers on an endpoint.
    pub(in crate::chat) listening: L,
    /// Whether a pane's shell is idle in a folder.
    pub(in crate::chat) idle: I,
}

/// What one run of the restore pass did.
#[derive(Debug, Default)]
pub(in crate::chat) struct Passed {
    /// Records claimed for this run, each with its line.
    pub(in crate::chat) claimed: Vec<(restore::Record, String)>,
    /// Where the next run starts reading, when this one left records or
    /// candidates unread, so none is passed over for good.
    pub(in crate::chat) next: Option<EndpointId>,
}

/// The restore pass over the records in `dir`, reading on `after` the
/// last run's stopping point: those [`plan`] keeps whose endpoint is gone
/// and whose pane is an idle shell in their folder, at most [`MAX_PROBES`]
/// of them asked about, each claimed for this run with its line. The `last`
/// run also removes the [`orphans`] and tidies the directory. Blocking.
pub(in crate::chat) fn pass(
    dir: &Path,
    after: Option<&EndpointId>,
    panes: &[Pane],
    boot: &str,
    program: &Path,
    last: bool,
    probes: Probes<
        impl Fn(&Path) -> bool,
        impl Fn(&EndpointId) -> bool,
        impl Fn(&Pane, &Path) -> bool,
    >,
) -> Passed {
    let listing = restore::list(dir, after);
    if last {
        for endpoint in orphans(&listing.records, panes, boot, &probes.owns) {
            restore::remove(dir, endpoint);
        }
        restore::prune(dir, std::time::SystemTime::now());
    }
    let candidates = plan(listing.records, panes, boot, &probes.owns);
    let next = if candidates.len() > MAX_PROBES {
        candidates
            .get(MAX_PROBES - 1)
            .map(|record| record.endpoint.clone())
    } else {
        listing.next
    };
    let claimed = candidates
        .into_iter()
        .take(MAX_PROBES)
        .filter(|record| !(probes.listening)(&record.endpoint))
        .filter(|record| {
            panes
                .iter()
                .find(|pane| pane.id == record.pane)
                .is_some_and(|pane| (probes.idle)(pane, &record.cwd))
        })
        .filter_map(|record| Some((line(program, &record)?, record)))
        // Claimed last, so a record no check refused is the only one taken.
        .filter(|(_, record)| restore::claim(dir, &record.endpoint).unwrap_or(false))
        .map(|(line, record)| (record, line))
        .collect();
    Passed { claimed, next }
}

/// Every record in `dir`, read once round the directory call by call; the
/// last call wraps to the start, so records already read are skipped.
/// Blocking.
pub(in crate::chat) fn every_record(dir: &Path) -> Vec<(restore::Record, Option<String>)> {
    let mut records = Vec::new();
    let mut read: HashSet<EndpointId> = HashSet::new();
    let mut after: Option<EndpointId> = None;
    let mut looked = 0;
    loop {
        let listing = restore::list(dir, after.as_ref());
        looked += listing.total.min(restore::MAX_RECORDS);
        records.extend(
            listing
                .records
                .into_iter()
                .filter(|(record, _)| read.insert(record.endpoint.clone())),
        );
        after = listing.next;
        if after.is_none() || looked >= listing.total {
            return records;
        }
    }
}
