//! The shared per-repository database. Opening it creates or migrates the
//! schema to [`SCHEMA_VERSION`]; a newer schema opens read-only. Every call
//! blocks on disk I/O, so it runs on the orchestrator's worker thread, never
//! on the UI thread.

use super::{
    Checkpoint, Error, HerdrSession, Item, ItemKey, Owner, Result, Run, SourceKey,
    model::{Activity, PullRequest, Workspace},
};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params};
use std::{path::Path, time::Duration};

/// The newest schema this build reads and writes.
pub(crate) const SCHEMA_VERSION: i64 = 1;

/// How long a write waits for the other application's transaction.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Version 0: agent-launcher's schema before the shared-state contract,
/// verbatim, so a database either application creates is complete.
pub(super) const SCHEMA_V0: &str = r#"
CREATE TABLE IF NOT EXISTS event_log (
    seq INTEGER PRIMARY KEY NOT NULL,
    at_unix_ms INTEGER NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS away_state (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    state_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS issues (
    canonical_key TEXT PRIMARY KEY NOT NULL,
    source TEXT NOT NULL,
    provider TEXT NOT NULL,
    host TEXT NOT NULL,
    repository TEXT NOT NULL,
    native_id TEXT NOT NULL,
    identifier TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    state TEXT NOT NULL,
    url TEXT,
    author TEXT,
    labels_json TEXT NOT NULL,
    parent_id TEXT,
    blocked_by_json TEXT NOT NULL,
    priority INTEGER,
    created_at TEXT,
    updated_at TEXT,
    pull_request_json TEXT,
    activity_json TEXT
);

CREATE INDEX IF NOT EXISTS issues_source_idx ON issues(source);

CREATE TABLE IF NOT EXISTS source_checkpoints (
    source TEXT PRIMARY KEY NOT NULL,
    checkpoint_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS security_advisory_cache (
    source TEXT PRIMARY KEY NOT NULL,
    issues_json TEXT NOT NULL,
    checkpoint_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY NOT NULL,
    issue_key TEXT NOT NULL,
    workspace_json TEXT,
    agent TEXT NOT NULL,
    model TEXT,
    state_json TEXT NOT NULL,
    message TEXT,
    session_id TEXT,
    started_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS runs_updated_idx ON runs(updated_at, id);

CREATE TABLE IF NOT EXISTS events (
    run_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    timestamp TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    PRIMARY KEY (run_id, sequence)
);

CREATE TABLE IF NOT EXISTS herdr_activity_samples (
    sampled_at_unix_ms INTEGER PRIMARY KEY NOT NULL,
    counts_json TEXT,
    expected_endpoints INTEGER NOT NULL CHECK (expected_endpoints >= 0),
    fresh_endpoints INTEGER NOT NULL CHECK (fresh_endpoints >= 0),
    stale_endpoints INTEGER NOT NULL CHECK (stale_endpoints >= 0),
    never_observed_endpoints INTEGER NOT NULL CHECK (never_observed_endpoints >= 0),
    failed_endpoints INTEGER NOT NULL CHECK (failed_endpoints >= 0),
    excluded_endpoints INTEGER NOT NULL CHECK (excluded_endpoints >= 0),
    inventory_complete INTEGER NOT NULL,
    completeness_json TEXT NOT NULL
);
"#;

const HERDR_SESSIONS_V1: &str = "CREATE TABLE IF NOT EXISTS herdr_sessions (
    run_id TEXT PRIMARY KEY NOT NULL,
    host TEXT,
    session TEXT,
    workspace_id TEXT NOT NULL,
    pane_id TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    updated_at TEXT NOT NULL
)";

const ITEM_COLUMNS: &str = "provider, host, repository, native_id, identifier, title, \
     description, state, url, author, labels_json, parent_id, blocked_by_json, priority, \
     created_at, updated_at, pull_request_json, activity_json";

const RUN_COLUMNS: &str = "id, issue_key, workspace_json, agent, model, state_json, message, \
     session_id, started_at, updated_at, owner";

/// What this build may do with the database.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Access {
    ReadWrite,
    /// The other application migrated past [`SCHEMA_VERSION`].
    ReadOnly {
        found: i64,
    },
}

pub(crate) struct Store {
    connection: Connection,
    access: Access,
}

impl Store {
    /// Opens `path`, creating the private directory and file when missing,
    /// and migrates it to [`SCHEMA_VERSION`].
    pub(crate) fn open(path: &Path) -> Result<Self> {
        private::prepare(path)?;
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        let access = migrate(&mut connection)?;
        Ok(Self { connection, access })
    }

    pub(crate) fn access(&self) -> Access {
        self.access
    }

    fn writable(&self) -> Result<()> {
        match self.access {
            Access::ReadWrite => Ok(()),
            Access::ReadOnly { found } => Err(Error::NewerSchema {
                found,
                known: SCHEMA_VERSION,
            }),
        }
    }

    /// Every cached item, of every source.
    pub(crate) fn items(&self) -> Result<Vec<Item>> {
        let mut statement = self
            .connection
            .prepare(&format!("SELECT {ITEM_COLUMNS} FROM issues"))?;
        let rows = statement.query_map([], |row| Ok(item_from_row(row)))?;
        rows.map(|row| row?).collect()
    }

    pub(crate) fn checkpoint(&self, source: &SourceKey) -> Result<Option<Checkpoint>> {
        self.connection
            .query_row(
                "SELECT checkpoint_json FROM source_checkpoints WHERE source = ?1",
                [source.canonical()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|json| decode("checkpoint_json", &json))
            .transpose()
    }

    /// Replaces every item of `source` and its checkpoint in one transaction:
    /// a full sync. Only a complete listing may be written this way.
    pub(crate) fn replace_items(
        &mut self,
        source: &SourceKey,
        items: &[Item],
        checkpoint: &Checkpoint,
    ) -> Result<()> {
        self.writable()?;
        let key = source.canonical();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM issues WHERE source = ?1", [&key])?;
        for item in items {
            insert_item(&transaction, &key, item)?;
        }
        transaction.execute(
            "INSERT INTO source_checkpoints (source, checkpoint_json) VALUES (?1, ?2) \
             ON CONFLICT(source) DO UPDATE SET checkpoint_json = excluded.checkpoint_json",
            params![key, encode("checkpoint_json", checkpoint)?],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Every non-confidential run, of either owner.
    pub(crate) fn runs(&self) -> Result<Vec<Run>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM runs ORDER BY started_at DESC, id"
        ))?;
        let rows = statement.query_map([], |row| Ok(run_from_row(row)))?;
        rows.map(|row| row?).collect()
    }

    pub(crate) fn sessions(&self) -> Result<Vec<HerdrSession>> {
        let mut statement = self.connection.prepare(
            "SELECT run_id, host, session, workspace_id, pane_id, agent_name, updated_at \
             FROM herdr_sessions",
        )?;
        let rows = statement.query_map([], |row| Ok(session_from_row(row)))?;
        rows.map(|row| row?).collect()
    }

    /// Writes a run herdr-gpui dispatched, with its Herdr session, in one
    /// transaction. Inserts or updates; a run owned elsewhere is refused.
    pub(crate) fn save_run(&mut self, run: &Run, session: Option<&HerdrSession>) -> Result<()> {
        self.writable()?;
        if run.owner != Owner::HerdrGpui {
            return Err(Error::NotOwner(run.id.clone()));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        refuse_foreign(&transaction, &run.id)?;
        let workspace = run
            .workspace
            .as_ref()
            .map(|workspace| encode("workspace_json", workspace))
            .transpose()?;
        transaction.execute(
            &format!(
                "INSERT INTO runs ({RUN_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
                 ON CONFLICT(id) DO UPDATE SET issue_key = excluded.issue_key, \
                 workspace_json = excluded.workspace_json, agent = excluded.agent, \
                 model = excluded.model, state_json = excluded.state_json, \
                 message = excluded.message, session_id = excluded.session_id, \
                 started_at = excluded.started_at, updated_at = excluded.updated_at"
            ),
            params![
                run.id,
                run.item_key,
                workspace,
                run.agent,
                run.model,
                encode("state_json", &run.state)?,
                run.message,
                run.session_id,
                time(run.started_at),
                time(run.updated_at),
                run.owner.as_str(),
            ],
        )?;
        match session {
            Some(session) => {
                transaction.execute(
                    "INSERT INTO herdr_sessions (run_id, host, session, workspace_id, pane_id, \
                     agent_name, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                     ON CONFLICT(run_id) DO UPDATE SET host = excluded.host, \
                     session = excluded.session, workspace_id = excluded.workspace_id, \
                     pane_id = excluded.pane_id, agent_name = excluded.agent_name, \
                     updated_at = excluded.updated_at",
                    params![
                        run.id,
                        session.host,
                        session.session,
                        session.workspace_id,
                        session.pane_id,
                        session.agent_name,
                        time(session.updated_at),
                    ],
                )?;
            }
            None => {
                transaction.execute("DELETE FROM herdr_sessions WHERE run_id = ?1", [&run.id])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Deletes a run herdr-gpui owns, with its events and session.
    pub(crate) fn delete_run(&mut self, id: &str) -> Result<()> {
        self.writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        refuse_foreign(&transaction, id)?;
        let deleted = transaction.execute("DELETE FROM runs WHERE id = ?1", [id])?;
        if deleted == 0 {
            return Err(Error::RunNotFound(id.to_owned()));
        }
        transaction.execute("DELETE FROM events WHERE run_id = ?1", [id])?;
        transaction.execute("DELETE FROM herdr_sessions WHERE run_id = ?1", [id])?;
        transaction.commit()?;
        Ok(())
    }
}

/// Fails when a run with `id` exists and another application owns it.
fn refuse_foreign(transaction: &Transaction<'_>, id: &str) -> Result<()> {
    let owner = transaction
        .query_row("SELECT owner FROM runs WHERE id = ?1", [id], |row| {
            row.get::<_, String>(0)
        })
        .optional()?;
    match owner.map(|owner| owner.parse::<Owner>()).transpose()? {
        Some(Owner::AgentLauncher) => Err(Error::NotOwner(id.to_owned())),
        Some(Owner::HerdrGpui) | None => Ok(()),
    }
}

/// Brings the schema to [`SCHEMA_VERSION`], or reports that it is newer.
fn migrate(connection: &mut Connection) -> Result<Access> {
    let version = user_version(connection)?;
    if version > SCHEMA_VERSION {
        return Ok(Access::ReadOnly { found: version });
    }
    if version < 1 {
        // SQLite cannot change the journal mode inside a transaction.
        connection.query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        })?;
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // The other application may have migrated while this one waited.
    let version = user_version(&transaction)?;
    if version > SCHEMA_VERSION {
        return Ok(Access::ReadOnly { found: version });
    }
    transaction.execute_batch(SCHEMA_V0)?;
    // agent-launcher's own column additions, in its order.
    if !has_column(&transaction, "issues", "pull_request_json")? {
        transaction.execute("ALTER TABLE issues ADD COLUMN pull_request_json TEXT", [])?;
        // Legacy GitHub checkpoints cover issues only; force a full PR bootstrap.
        transaction.execute(
            "DELETE FROM source_checkpoints WHERE source GLOB 'github:*'",
            [],
        )?;
    }
    if !has_column(&transaction, "issues", "activity_json")? {
        transaction.execute("ALTER TABLE issues ADD COLUMN activity_json TEXT", [])?;
    }
    if !has_column(&transaction, "runs", "model")? {
        transaction.execute("ALTER TABLE runs ADD COLUMN model TEXT", [])?;
    }
    if version < 1 {
        if !has_column(&transaction, "runs", "owner")? {
            transaction.execute(
                "ALTER TABLE runs ADD COLUMN owner TEXT NOT NULL DEFAULT 'agent-launcher'",
                [],
            )?;
        }
        transaction.execute(HERDR_SESSIONS_V1, [])?;
        transaction.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
    }
    transaction.commit()?;
    Ok(Access::ReadWrite)
}

fn user_version(connection: &Connection) -> Result<i64> {
    Ok(connection.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = statement.query_map([], |row| row.get::<_, String>("name"))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn insert_item(transaction: &Transaction<'_>, source: &str, item: &Item) -> Result<()> {
    let key = &item.key;
    transaction.execute(
        &format!(
            "INSERT INTO issues (canonical_key, source, {ITEM_COLUMNS}) VALUES (?1, ?2, ?3, ?4, \
             ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20) \
             ON CONFLICT(canonical_key) DO UPDATE SET source = excluded.source, \
             provider = excluded.provider, host = excluded.host, \
             repository = excluded.repository, native_id = excluded.native_id, \
             identifier = excluded.identifier, title = excluded.title, \
             description = excluded.description, state = excluded.state, url = excluded.url, \
             author = excluded.author, labels_json = excluded.labels_json, \
             parent_id = excluded.parent_id, blocked_by_json = excluded.blocked_by_json, \
             priority = excluded.priority, created_at = excluded.created_at, \
             updated_at = excluded.updated_at, pull_request_json = excluded.pull_request_json, \
             activity_json = excluded.activity_json"
        ),
        params![
            key.canonical(),
            source,
            key.source.provider.as_str(),
            key.source.host,
            key.source.repository,
            key.native_id,
            item.identifier,
            item.title,
            item.description,
            item.state,
            item.url,
            item.author,
            encode("labels_json", &item.labels)?,
            item.parent_id,
            encode("blocked_by_json", &item.blocked_by)?,
            item.priority,
            item.created_at.map(time),
            item.updated_at.map(time),
            item.pull_request
                .as_ref()
                .map(|pr| encode("pull_request_json", pr))
                .transpose()?,
            item.activity
                .as_ref()
                .map(|activity| encode("activity_json", activity))
                .transpose()?,
        ],
    )?;
    Ok(())
}

fn item_from_row(row: &Row<'_>) -> Result<Item> {
    let provider: String = row.get("provider")?;
    Ok(Item {
        key: ItemKey {
            source: SourceKey {
                provider: provider.parse()?,
                host: row.get("host")?,
                repository: row.get("repository")?,
            },
            native_id: row.get("native_id")?,
        },
        identifier: row.get("identifier")?,
        title: row.get("title")?,
        description: row.get("description")?,
        state: row.get("state")?,
        url: row.get("url")?,
        author: row.get("author")?,
        labels: decode("labels_json", &row.get::<_, String>("labels_json")?)?,
        parent_id: row.get("parent_id")?,
        blocked_by: decode("blocked_by_json", &row.get::<_, String>("blocked_by_json")?)?,
        priority: row.get("priority")?,
        created_at: optional_time(row, "created_at")?,
        updated_at: optional_time(row, "updated_at")?,
        pull_request: optional_json::<PullRequest>(row, "pull_request_json")?,
        activity: optional_json::<Activity>(row, "activity_json")?,
    })
}

fn run_from_row(row: &Row<'_>) -> Result<Run> {
    Ok(Run {
        id: row.get("id")?,
        item_key: row.get("issue_key")?,
        workspace: optional_json::<Workspace>(row, "workspace_json")?,
        agent: row.get("agent")?,
        model: row.get("model")?,
        state: decode("state_json", &row.get::<_, String>("state_json")?)?,
        message: row.get("message")?,
        session_id: row.get("session_id")?,
        started_at: parse_time("started_at", row.get("started_at")?)?,
        updated_at: parse_time("updated_at", row.get("updated_at")?)?,
        owner: row.get::<_, String>("owner")?.parse()?,
    })
}

fn session_from_row(row: &Row<'_>) -> Result<HerdrSession> {
    Ok(HerdrSession {
        run_id: row.get("run_id")?,
        host: row.get("host")?,
        session: row.get("session")?,
        workspace_id: row.get("workspace_id")?,
        pane_id: row.get("pane_id")?,
        agent_name: row.get("agent_name")?,
        updated_at: parse_time("updated_at", row.get("updated_at")?)?,
    })
}

fn encode(column: &'static str, value: &impl serde::Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|source| Error::Json { column, source })
}

fn decode<T: serde::de::DeserializeOwned>(column: &'static str, json: &str) -> Result<T> {
    serde_json::from_str(json).map_err(|source| Error::Json { column, source })
}

fn optional_json<T: serde::de::DeserializeOwned>(
    row: &Row<'_>,
    column: &'static str,
) -> Result<Option<T>> {
    row.get::<_, Option<String>>(column)?
        .map(|json| decode(column, &json))
        .transpose()
}

/// Times as agent-launcher's SQLite driver writes them: RFC 3339 with `+00:00`.
fn time(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

fn parse_time(column: &'static str, value: String) -> Result<DateTime<Utc>> {
    match DateTime::parse_from_rfc3339(&value) {
        Ok(time) => Ok(time.with_timezone(&Utc)),
        Err(source) => Err(Error::Time {
            column,
            value,
            source,
        }),
    }
}

fn optional_time(row: &Row<'_>, column: &'static str) -> Result<Option<DateTime<Utc>>> {
    row.get::<_, Option<String>>(column)?
        .map(|value| parse_time(column, value))
        .transpose()
}

/// Creating the database directory and file privately, as agent-launcher does.
mod private {
    use super::{Error, Result};
    use std::{fs, io, path::Path};

    pub(super) fn prepare(path: &Path) -> Result<()> {
        let prepare = |path: &Path| {
            let path = path.to_owned();
            move |source| Error::Prepare { path, source }
        };
        let directory = path.parent().ok_or_else(|| Error::NotPrivate {
            path: path.to_owned(),
        })?;
        if let Some(parent) = directory.parent() {
            fs::create_dir_all(parent).map_err(prepare(parent))?;
        }
        match create_directory(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(prepare(directory)(error)),
        }
        check(directory, true)?;
        match create_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(prepare(path)(error)),
        }
        check(path, false)
    }

    #[cfg(unix)]
    fn create_directory(path: &Path) -> io::Result<()> {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    }

    #[cfg(not(unix))]
    fn create_directory(path: &Path) -> io::Result<()> {
        fs::create_dir(path)
    }

    #[cfg(unix)]
    fn create_file(path: &Path) -> io::Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map(drop)
    }

    #[cfg(not(unix))]
    fn create_file(path: &Path) -> io::Result<()> {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map(drop)
    }

    /// The directory or file must be ours, not a link, and the file must have
    /// one name. Permissions are not widened or repaired.
    fn check(path: &Path, directory: bool) -> Result<()> {
        let metadata = fs::symlink_metadata(path).map_err(|source| Error::Prepare {
            path: path.to_owned(),
            source,
        })?;
        let kind = if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        };
        if !kind || metadata.file_type().is_symlink() || !owned(&metadata, directory) {
            return Err(Error::NotPrivate {
                path: path.to_owned(),
            });
        }
        Ok(())
    }

    #[cfg(unix)]
    fn owned(metadata: &fs::Metadata, directory: bool) -> bool {
        use std::os::unix::fs::MetadataExt;
        metadata.uid() == rustix::process::getuid().as_raw() && (directory || metadata.nlink() == 1)
    }

    #[cfg(not(unix))]
    fn owned(_: &fs::Metadata, _: bool) -> bool {
        true
    }
}
