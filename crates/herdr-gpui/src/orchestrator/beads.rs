//! Beads issues, read with the `bd` CLI in the main checkout on its host,
//! exactly as agent-launcher reads them: `bd list --json --all --limit 0` and
//! `bd blocked --json`, read-only and sandboxed when `bd` supports it, with
//! `BEADS_DIR` pinned to the checkout.

use super::{Checkpoint, Error, Item, ItemKey, Result, SourceKey, error::script};
use crate::teleport::Host;
use chrono::{DateTime, Utc};
use herdr_client::shell_quote;
use serde::Deserialize;
use std::{collections::HashMap, sync::atomic::AtomicBool};

/// Exit status the scripts use when `bd` is not on the host's `PATH`.
const MISSING: i32 = 127;

/// Reads every bead. Beads always syncs fully.
pub(crate) fn sync(
    host: &Host,
    source: &SourceKey,
    cancelled: &AtomicBool,
) -> Result<(Vec<Item>, Checkpoint)> {
    let flags = safety_flags(host, &source.repository, cancelled)?;
    let list = run(
        host,
        &source.repository,
        &flags,
        &["list", "--json", "--all", "--limit", "0"],
        cancelled,
    )?;
    let blocked = run(
        host,
        &source.repository,
        &flags,
        &["blocked", "--json"],
        cancelled,
    )?;
    let items = parse(source, &list, &blocked)?;
    let checkpoint = Checkpoint {
        updated_at: items.iter().filter_map(|item| item.updated_at).max(),
        last_full_at: Some(Utc::now()),
        ..Checkpoint::default()
    };
    Ok((items, checkpoint))
}

/// Deletes one bead for good, which `bd` cannot do read-only.
pub(crate) fn delete(
    host: &Host,
    source: &SourceKey,
    id: &str,
    cancelled: &AtomicBool,
) -> Result<()> {
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

pub(super) fn valid_id(id: &str) -> bool {
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

fn safety_flags(host: &Host, root: &str, cancelled: &AtomicBool) -> Result<Vec<&'static str>> {
    let body = format!("{}bd --help 2>&1\n", prefix(root));
    let help = capture(host, &body, "Reading bd's options", cancelled)?;
    Ok(flags_from_help(&String::from_utf8_lossy(&help)))
}

pub(super) fn flags_from_help(help: &str) -> Vec<&'static str> {
    let words: Vec<_> = help.split_whitespace().collect();
    ["--readonly", "--sandbox"]
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
    let command = flags
        .iter()
        .chain(args)
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let body = format!("{}bd {command}\n", prefix(root));
    capture(host, &body, "Reading Beads", cancelled)
}

fn capture(
    host: &Host,
    body: &str,
    operation: &'static str,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>> {
    host.capture(body, cancelled).map_err(|error| match error {
        herdr_client::Error::ScriptExit { status, .. } if status.code() == Some(MISSING) => {
            Error::BeadsMissing
        }
        error => script(operation)(error),
    })
}

pub(super) fn parse(source: &SourceKey, list: &[u8], blocked: &[u8]) -> Result<Vec<Item>> {
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
