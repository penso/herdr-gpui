//! Beads issues, read with the `bd` CLI in the main checkout on its host,
//! exactly as agent-launcher reads them: `bd list --json --all --limit 0` and
//! `bd blocked --json`, read-only and sandboxed when `bd` supports it, with
//! `BEADS_DIR` pinned to the checkout.

use super::{Checkpoint, Error, Item, ItemKey, Result, SourceKey, error::script};
use chrono::{DateTime, Utc};
use herdr_client::{ScriptLimits, shell_quote};
use herdr_remote::teleport::Host;
use serde::Deserialize;
use std::{collections::HashMap, sync::atomic::AtomicBool};

/// Exit status the scripts use when `bd` is not on the host's `PATH`.
const MISSING: i32 = 127;

/// Reads every bead. Beads always syncs fully.
pub fn sync(
    host: &Host,
    source: &SourceKey,
    cancelled: &AtomicBool,
) -> Result<(Vec<Item>, Checkpoint)> {
    let (list, blocked) = if host.is_local() {
        let flags = safety_flags(host, &source.repository, cancelled)?;
        let list = run(host, &source.repository, &flags, &LIST, cancelled)?;
        let blocked = run(host, &source.repository, &flags, &BLOCKED, cancelled)?;
        (list, blocked)
    } else {
        // One connection rather than three: sshd may drop a burst of them.
        let separator = super::prompt::nonce();
        let body = sync_script(&source.repository, &separator);
        let mut output = Vec::new();
        let limits = ScriptLimits {
            output: SYNC_OUTPUT,
            idle: Host::QUERY_IDLE,
        };
        host.stream(&body, &[][..], &mut output, limits, cancelled)
            .map_err(failed("Reading Beads"))?;
        split(&output, &separator)?
    };
    let items = parse(source, &list, &blocked)?;
    let checkpoint = Checkpoint {
        updated_at: items.iter().filter_map(|item| item.updated_at).max(),
        last_full_at: Some(Utc::now()),
        ..Checkpoint::default()
    };
    Ok((items, checkpoint))
}

/// Most [`sync_script`] may print: room for the list and the blocked beads
/// at the limit each had as its own script, and the lines around them.
pub const SYNC_OUTPUT: u64 = 2 * Host::QUERY_OUTPUT + 4096;

const LIST: [&str; 5] = ["list", "--json", "--all", "--limit", "0"];
const BLOCKED: [&str; 2] = ["blocked", "--json"];

/// What [`sync`] runs on an SSH host in one connection: `bd`'s options read
/// as [`flags_from_help`] reads them, printed on a `flags` line, then the
/// list and the blocked beads as `bd` prints them, with a `separator` line
/// between them. Each step prints as soon as it ends, so each has the idle
/// limit to itself, as in its own script. Any step's failure ends the
/// script with its status, as its own script would.
pub fn sync_script(root: &str, separator: &str) -> String {
    let words = |args: &[&str]| {
        args.iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let flags = FLAGS.join(" ");
    let (list, blocked) = (words(&LIST), words(&BLOCKED));
    let separator = shell_quote(separator);
    format!(
        r#"{prefix}help=$(bd --help 2>&1) || exit $?
words=" $(printf '%s' "$help" | tr '\n\t\r' '   ') "
flags=
for flag in {flags}; do
    case $words in *" $flag "*) flags="$flags $flag" ;; esac
done
printf 'flags%s\n' "$flags"
bd $flags {list} || exit $?
printf '\n%s\n' {separator}
bd $flags {blocked} || exit $?
"#,
        prefix = prefix(root)
    )
}

/// The list and the blocked beads in [`sync_script`]'s `output`.
pub fn split(output: &[u8], separator: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let unexpected = || Error::Output {
        operation: "Reading Beads",
    };
    let output = output
        .strip_prefix(b"flags")
        .and_then(|rest| {
            rest.iter()
                .position(|&byte| byte == b'\n')
                .map(|end| &rest[end..])
        })
        .ok_or_else(unexpected)?;
    let line = format!("\n{separator}\n");
    let at = output
        .windows(line.len())
        .position(|window| window == line.as_bytes())
        .ok_or_else(unexpected)?;
    Ok((output[..at].to_vec(), output[at + line.len()..].to_vec()))
}

/// Deletes one bead for good, which `bd` cannot do read-only.
pub fn delete(host: &Host, source: &SourceKey, id: &str, cancelled: &AtomicBool) -> Result<()> {
    if !valid_id(id) {
        return Err(Error::BeadId(id.to_owned()));
    }
    let mut flags = safety_flags(host, &source.repository, cancelled)?;
    flags.retain(|flag| *flag != "--readonly");
    run(
        host,
        &source.repository,
        &flags,
        &["delete", "--force", "--", id],
        cancelled,
    )
    .map(drop)
}

pub fn valid_id(id: &str) -> bool {
    id.len() <= 255
        && id.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// The script prefix running `bd` in `root` with Beads' routing pinned.
fn prefix(root: &str) -> String {
    let root = shell_quote(root);
    format!(
        "cd -- {root}\ncommand -v bd >/dev/null 2>&1 || exit {MISSING}\n\
         BEADS_DIR={root}/.beads; export BEADS_DIR; unset BEADS_DB\n"
    )
}

/// The safety flags `bd` is run with when its help names them.
const FLAGS: [&str; 2] = ["--readonly", "--sandbox"];

fn safety_flags(host: &Host, root: &str, cancelled: &AtomicBool) -> Result<Vec<&'static str>> {
    const READING: &str = "Reading bd's options";
    // `bd`'s help goes to stdout, which is all a local run reads.
    let help = if host.is_local() {
        local(host, root, &["--help"], READING, cancelled)?
    } else {
        let body = format!("{}bd --help 2>&1\n", prefix(root));
        capture(host, &body, READING, cancelled)?
    };
    Ok(flags_from_help(&String::from_utf8_lossy(&help)))
}

pub fn flags_from_help(help: &str) -> Vec<&'static str> {
    let words: Vec<_> = help.split_whitespace().collect();
    FLAGS
        .into_iter()
        .filter(|flag| words.contains(flag))
        .collect()
}

fn run(
    host: &Host,
    root: &str,
    flags: &[&str],
    args: &[&str],
    cancelled: &AtomicBool,
) -> Result<Vec<u8>> {
    if host.is_local() {
        let args: Vec<&str> = flags.iter().chain(args).copied().collect();
        return local(host, root, &args, "Reading Beads", cancelled);
    }
    let command = flags
        .iter()
        .chain(args)
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let body = format!("{}bd {command}\n", prefix(root));
    capture(host, &body, "Reading Beads", cancelled)
}

/// `bd` with `args` run directly in `root` on this machine, with Beads'
/// routing pinned as [`prefix`] pins it.
fn local(
    host: &Host,
    root: &str,
    args: &[&str],
    operation: &'static str,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>> {
    if !host.has_local("bd") {
        return Err(Error::BeadsMissing);
    }
    let mut command = host.local_command("bd");
    command
        .args(args)
        .current_dir(root)
        .env("BEADS_DIR", std::path::Path::new(root).join(".beads"))
        .env_remove("BEADS_DB");
    Host::capture_command(command, Host::QUERY_IDLE, cancelled).map_err(script(operation))
}

fn capture(
    host: &Host,
    body: &str,
    operation: &'static str,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>> {
    host.capture(body, cancelled).map_err(failed(operation))
}

/// `map_err` adapter for a Beads script: `bd` missing, or `operation` failed.
fn failed(operation: &'static str) -> impl FnOnce(herdr_client::Error) -> Error {
    move |error| match error {
        herdr_client::Error::ScriptExit { status, .. } if status.code() == Some(MISSING) => {
            Error::BeadsMissing
        }
        error => script(operation)(error),
    }
}

pub fn parse(source: &SourceKey, list: &[u8], blocked: &[u8]) -> Result<Vec<Item>> {
    let json = |operation| move |source| Error::OutputJson { operation, source };
    let records: Output<Bead> = serde_json::from_slice(list).map_err(json("Reading Beads"))?;
    let blockers: Output<Blocked> =
        serde_json::from_slice(blocked).map_err(json("Reading Beads blockers"))?;
    let mut blockers: HashMap<String, Vec<String>> = blockers
        .into_records()
        .into_iter()
        .map(|record| {
            let ids = record
                .blocked_by
                .into_iter()
                .map(Reference::into_id)
                .collect();
            (record.id, ids)
        })
        .collect();
    Ok(records
        .into_records()
        .into_iter()
        .map(|record| {
            let blocked_by = blockers.remove(&record.id).unwrap_or_default();
            record.into_item(source, blocked_by)
        })
        .collect())
}

/// `bd` prints either a bare list or `{"issues": [...]}`.
#[derive(Deserialize)]
#[serde(untagged)]
enum Output<T> {
    List(Vec<T>),
    Wrapped { issues: Vec<T> },
}

impl<T> Output<T> {
    fn into_records(self) -> Vec<T> {
        match self {
            Self::List(records) | Self::Wrapped { issues: records } => records,
        }
    }
}

#[derive(Deserialize)]
struct Blocked {
    id: String,
    #[serde(default)]
    blocked_by: Vec<Reference>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Reference {
    Id(String),
    Object {
        #[serde(alias = "depends_on_id", alias = "issue_id")]
        id: String,
    },
}

impl Reference {
    fn into_id(self) -> String {
        match self {
            Self::Id(id) | Self::Object { id } => id,
        }
    }
}

#[derive(Deserialize)]
struct Dependency {
    depends_on_id: String,
    #[serde(rename = "type", alias = "dependency_type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct Bead {
    id: String,
    title: String,
    description: Option<String>,
    status: String,
    priority: Option<i64>,
    created_by: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(alias = "parent")]
    parent_id: Option<String>,
    #[serde(default)]
    dependencies: Vec<Dependency>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
}

impl Bead {
    fn into_item(self, source: &SourceKey, mut blocked_by: Vec<String>) -> Item {
        let mut parent_id = self.parent_id;
        for dependency in self.dependencies {
            if matches!(dependency.kind.as_deref(), Some("parent" | "parent-child")) {
                parent_id.get_or_insert(dependency.depends_on_id);
            }
        }
        blocked_by.sort();
        blocked_by.dedup();
        Item {
            key: ItemKey {
                source: source.clone(),
                native_id: self.id.clone(),
            },
            identifier: self.id,
            title: self.title,
            description: self.description,
            state: self.status,
            url: None,
            author: self.created_by,
            labels: self.labels,
            parent_id,
            blocked_by,
            priority: self.priority,
            created_at: self.created_at,
            updated_at: self.updated_at,
            pull_request: None,
            activity: None,
        }
    }
}
