//! The daemon's state in SQLite, shared with the command line: the daemon
//! writes what it sees and does, commands write the user's choices and
//! requests, and both read. WAL mode and a busy timeout let them work at
//! the same time; a unique firing key keeps any firing from running twice.

use crate::Result;
use rusqlite::{Connection, OptionalExtension, Row, params};
use sheprd_automation::report::{Reason, RunState, Tokens, TriggerStatus};
use std::{path::Path, time::Duration};

mod rows;

pub use rows::{
    AgentRow, DaemonRow, NewRun, Request, RequestKind, RunRow, ScriptRow, StepRow, Switch,
};

const SCHEMA_VERSION: i64 = 1;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS repos (path TEXT PRIMARY KEY, added_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS approvals (
    path TEXT PRIMARY KEY, hash TEXT NOT NULL, approved_at INTEGER NOT NULL, by TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS switches (
    path TEXT PRIMARY KEY, paused INTEGER NOT NULL DEFAULT 0,
    disabled INTEGER NOT NULL DEFAULT 0, reason TEXT);
CREATE TABLE IF NOT EXISTS scripts (
    path TEXT PRIMARY KEY, name TEXT NOT NULL, repo TEXT, hash TEXT NOT NULL,
    error TEXT, plan TEXT, triggers TEXT, caps TEXT, seen_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS cursors (
    path TEXT NOT NULL, trigger INTEGER NOT NULL, checked INTEGER NOT NULL, fired INTEGER,
    PRIMARY KEY (path, trigger));
CREATE TABLE IF NOT EXISTS runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT, path TEXT NOT NULL, name TEXT NOT NULL,
    trigger INTEGER NOT NULL, trigger_name TEXT, reason TEXT NOT NULL, scheduled INTEGER,
    hash TEXT NOT NULL, started_at INTEGER NOT NULL, finished_at INTEGER,
    state TEXT NOT NULL, detail TEXT, steps INTEGER NOT NULL DEFAULT 0,
    UNIQUE (path, trigger, scheduled));
CREATE INDEX IF NOT EXISTS runs_by_path ON runs (path, id);
CREATE TABLE IF NOT EXISTS steps (
    run INTEGER NOT NULL, position INTEGER NOT NULL, kind TEXT NOT NULL, summary TEXT NOT NULL,
    started_at INTEGER NOT NULL, finished_at INTEGER, state TEXT NOT NULL, exit INTEGER,
    log_tail TEXT NOT NULL DEFAULT '', worktree TEXT, pane TEXT,
    cpu_ms INTEGER, peak_memory INTEGER, PRIMARY KEY (run, position));
CREATE TABLE IF NOT EXISTS agents (
    run INTEGER NOT NULL, position INTEGER NOT NULL, path TEXT NOT NULL, harness TEXT NOT NULL,
    pane TEXT NOT NULL, cwd TEXT NOT NULL, started_at INTEGER NOT NULL, done_at INTEGER,
    input INTEGER, output INTEGER, cache_read INTEGER, cache_write INTEGER,
    cpu_ms INTEGER, peak_memory INTEGER, sampled_at INTEGER, PRIMARY KEY (run, position));
CREATE TABLE IF NOT EXISTS agent_days (
    run INTEGER NOT NULL, position INTEGER NOT NULL, day INTEGER NOT NULL,
    input INTEGER NOT NULL, output INTEGER NOT NULL, cache_read INTEGER NOT NULL,
    cache_write INTEGER NOT NULL, PRIMARY KEY (run, position, day));
CREATE TABLE IF NOT EXISTS requests (
    id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, path TEXT, argument TEXT,
    created_at INTEGER NOT NULL, taken_at INTEGER);
CREATE TABLE IF NOT EXISTS daemon (
    id INTEGER PRIMARY KEY CHECK (id = 1), pid INTEGER, version TEXT, started_at INTEGER,
    beat_at INTEGER, paused_all INTEGER NOT NULL DEFAULT 0);
";

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            crate::paths::private_dir(parent).map_err(crate::Error::io(parent))?;
        }
        // Create it private before SQLite does; its WAL files follow suit.
        crate::paths::private_file()
            .create(true)
            .append(true)
            .open(path)
            .map_err(crate::Error::io(path))?;
        let connection = Connection::open(path)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        connection.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        connection.execute_batch(SCHEMA)?;
        let version = connection
            .query_row("SELECT value FROM meta WHERE key = 'schema'", [], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        match version.map(|version| version.parse::<i64>()) {
            None => {
                connection.execute(
                    "INSERT INTO meta (key, value) VALUES ('schema', ?1)",
                    params![SCHEMA_VERSION.to_string()],
                )?;
            }
            Some(Ok(SCHEMA_VERSION)) => {}
            Some(Ok(found)) => return Err(crate::Error::Schema { found }),
            Some(Err(_)) => return Err(crate::Error::Schema { found: -1 }),
        }
        Ok(Self { connection })
    }

    // Repositories

    pub fn repos(&self) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT path FROM repos ORDER BY path")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn add_repo(&self, path: &str, now: i64) -> Result<bool> {
        let added = self.connection.execute(
            "INSERT OR IGNORE INTO repos (path, added_at) VALUES (?1, ?2)",
            params![path, now],
        )?;
        Ok(added > 0)
    }

    pub fn remove_repo(&self, path: &str) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM repos WHERE path = ?1", params![path])?
            > 0)
    }

    // Approvals and the user's switches

    pub fn approved_hash(&self, path: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT hash FROM approvals WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn approve(&self, path: &str, hash: &str, by: &str, now: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO approvals (path, hash, approved_at, by) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (path) DO UPDATE SET hash = ?2, approved_at = ?3, by = ?4",
            params![path, hash, now, by],
        )?;
        Ok(())
    }

    pub fn switch(&self, path: &str) -> Result<Switch> {
        Ok(self
            .connection
            .query_row(
                "SELECT paused, disabled, reason FROM switches WHERE path = ?1",
                params![path],
                |row| {
                    Ok(Switch {
                        paused: row.get(0)?,
                        disabled: row.get(1)?,
                        reason: row.get(2)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }

    pub fn set_switch(&self, path: &str, switch: &Switch) -> Result<()> {
        self.connection.execute(
            "INSERT INTO switches (path, paused, disabled, reason) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (path) DO UPDATE SET paused = ?2, disabled = ?3, reason = ?4",
            params![path, switch.paused, switch.disabled, switch.reason],
        )?;
        Ok(())
    }

    // Scripts as the daemon last saw them

    pub fn put_script(&self, script: &ScriptRow) -> Result<()> {
        self.connection.execute(
            "INSERT INTO scripts (path, name, repo, hash, error, plan, triggers, caps, seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT (path) DO UPDATE SET name = ?2, repo = ?3, hash = ?4, error = ?5,
                 plan = ?6, triggers = ?7, caps = ?8, seen_at = ?9",
            params![
                script.path,
                script.name,
                script.repo,
                script.hash,
                script.error,
                script.plan,
                serde_json::to_string(&script.triggers)?,
                script
                    .caps
                    .map(|caps| serde_json::to_string(&caps))
                    .transpose()?,
                script.seen_at
            ],
        )?;
        Ok(())
    }

    /// Records when each trigger fires next, as the scheduler sees it.
    pub fn set_triggers(&self, path: &str, triggers: &[TriggerStatus]) -> Result<()> {
        self.connection.execute(
            "UPDATE scripts SET triggers = ?2 WHERE path = ?1",
            params![path, serde_json::to_string(triggers)?],
        )?;
        Ok(())
    }

    /// Forgets scripts whose files were not seen in the latest scan.
    pub fn forget_scripts_before(&self, seen_at: i64) -> Result<usize> {
        Ok(self
            .connection
            .execute("DELETE FROM scripts WHERE seen_at < ?1", params![seen_at])?)
    }

    pub fn scripts(&self) -> Result<Vec<ScriptRow>> {
        let mut statement = self.connection.prepare(
            "SELECT path, name, repo, hash, error, plan, triggers, caps, seen_at FROM scripts ORDER BY path",
        )?;
        let rows = statement.query_map([], ScriptRow::read)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn script(&self, path: &str) -> Result<Option<ScriptRow>> {
        Ok(self
            .connection
            .query_row(
                "SELECT path, name, repo, hash, error, plan, triggers, caps, seen_at FROM scripts WHERE path = ?1",
                params![path],
                ScriptRow::read,
            )
            .optional()?)
    }

    // Scheduling cursors

    /// When trigger `index` was last checked, and its anchor: when it last
    /// fired, or when it started counting.
    pub fn cursor(&self, path: &str, index: usize) -> Result<Option<(i64, Option<i64>)>> {
        Ok(self
            .connection
            .query_row(
                "SELECT checked, fired FROM cursors WHERE path = ?1 AND trigger = ?2",
                params![path, index_value(index)],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    pub fn set_cursor(
        &self,
        path: &str,
        index: usize,
        checked: i64,
        fired: Option<i64>,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO cursors (path, trigger, checked, fired) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (path, trigger) DO UPDATE SET checked = ?3, fired = COALESCE(?4, fired)",
            params![path, index_value(index), checked, fired],
        )?;
        Ok(())
    }

    pub fn clear_cursors(&self, path: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM cursors WHERE path = ?1", params![path])?;
        Ok(())
    }

    // Runs

    /// Records a run as started. `None` when that firing already has a run,
    /// so a firing never runs twice.
    pub fn start_run(&self, run: &NewRun) -> Result<Option<i64>> {
        let inserted = self.connection.execute(
            "INSERT OR IGNORE INTO runs
                 (path, name, trigger, trigger_name, reason, scheduled, hash, started_at, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'running')",
            params![
                run.path,
                run.name,
                index_value(run.trigger),
                run.trigger_name,
                rows::reason_name(run.reason),
                run.scheduled,
                run.hash,
                run.started_at
            ],
        )?;
        Ok((inserted > 0).then(|| self.connection.last_insert_rowid()))
    }

    pub fn finish_run(
        &self,
        id: i64,
        state: RunState,
        detail: Option<&str>,
        now: i64,
    ) -> Result<()> {
        self.connection.execute(
            "UPDATE runs SET state = ?2, detail = ?3, finished_at = ?4 WHERE id = ?1",
            params![id, rows::run_state_name(state), detail, now],
        )?;
        Ok(())
    }

    /// Marks runs a previous daemon left running as interrupted; they are
    /// not resumed.
    pub fn interrupt_running(&self, now: i64) -> Result<usize> {
        self.connection.execute(
            "UPDATE steps SET state = 'halted', finished_at = ?1 WHERE state = 'running'",
            params![now],
        )?;
        Ok(self.connection.execute(
            "UPDATE runs SET state = 'interrupted', finished_at = ?1,
                 detail = 'the daemon stopped while it ran' WHERE state = 'running'",
            params![now],
        )?)
    }

    pub fn runs(&self, path: Option<&str>, limit: usize) -> Result<Vec<RunRow>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self.connection.prepare(&format!(
            "SELECT {} FROM runs WHERE (?1 IS NULL OR path = ?1) ORDER BY id DESC LIMIT ?2",
            rows::RUN_COLUMNS
        ))?;
        let rows = statement.query_map(params![path, limit], RunRow::read)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn run(&self, id: i64) -> Result<Option<RunRow>> {
        Ok(self
            .connection
            .query_row(
                &format!("SELECT {} FROM runs WHERE id = ?1", rows::RUN_COLUMNS),
                params![id],
                RunRow::read,
            )
            .optional()?)
    }

    pub fn running(&self, path: &str) -> Result<Option<i64>> {
        Ok(self
            .connection
            .query_row(
                "SELECT id FROM runs WHERE path = ?1 AND state = 'running' ORDER BY id DESC LIMIT 1",
                params![path],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Finished runs of a script, newest first, until one that did not fail.
    pub fn failures_in_a_row(&self, path: &str) -> Result<u32> {
        let mut statement = self.connection.prepare(
            "SELECT state FROM runs WHERE path = ?1 AND state != 'running' ORDER BY id DESC LIMIT 20",
        )?;
        let states = statement.query_map(params![path], |row| row.get::<_, String>(0))?;
        let mut count = 0;
        for state in states {
            match state?.as_str() {
                "failed" | "halted" | "stopped" => count += 1,
                _ => break,
            }
        }
        Ok(count)
    }

    // Steps

    pub fn start_step(
        &self,
        run: i64,
        position: u32,
        kind: &str,
        summary: &str,
        now: i64,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO steps (run, position, kind, summary, started_at, state)
             VALUES (?1, ?2, ?3, ?4, ?5, 'running')",
            params![run, position, kind, summary, now],
        )?;
        self.connection.execute(
            "UPDATE runs SET steps = MAX(steps, ?2) WHERE id = ?1",
            params![run, position],
        )?;
        Ok(())
    }

    pub fn finish_step(&self, run: i64, position: u32, step: &StepRow, now: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE steps SET state = ?3, exit = ?4, log_tail = ?5, worktree = ?6, pane = ?7,
                 cpu_ms = ?8, peak_memory = ?9, finished_at = ?10
             WHERE run = ?1 AND position = ?2",
            params![
                run,
                position,
                rows::step_state_name(step.state),
                step.exit,
                step.log_tail,
                step.worktree,
                step.pane,
                step.cpu_ms.and_then(|value| i64::try_from(value).ok()),
                step.peak_memory.and_then(|value| i64::try_from(value).ok()),
                now
            ],
        )?;
        Ok(())
    }

    pub fn steps(&self, run: i64) -> Result<Vec<StepRow>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {} FROM steps WHERE run = ?1 ORDER BY position",
            rows::STEP_COLUMNS
        ))?;
        let rows = statement.query_map(params![run], StepRow::read)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // Agents and their usage

    pub fn add_agent(&self, agent: &AgentRow) -> Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO agents (run, position, path, harness, pane, cwd, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                agent.run,
                agent.position,
                agent.path,
                agent.harness,
                agent.pane,
                agent.cwd,
                agent.started_at
            ],
        )?;
        Ok(())
    }

    pub fn agents(&self, live_only: bool) -> Result<Vec<AgentRow>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {} FROM agents WHERE (?1 = 0 OR done_at IS NULL) ORDER BY run, position",
            rows::AGENT_COLUMNS
        ))?;
        let rows = statement.query_map(params![live_only], AgentRow::read)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn run_agents(&self, run: i64) -> Result<Vec<AgentRow>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {} FROM agents WHERE run = ?1 ORDER BY position",
            rows::AGENT_COLUMNS
        ))?;
        let rows = statement.query_map(params![run], AgentRow::read)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn update_agent(&self, agent: &AgentRow) -> Result<()> {
        let tokens = agent.tokens;
        let count = |value: Option<u64>| value.and_then(|value| i64::try_from(value).ok());
        self.connection.execute(
            "UPDATE agents SET input = ?3, output = ?4, cache_read = ?5, cache_write = ?6,
                 cpu_ms = ?7, peak_memory = ?8, sampled_at = ?9, done_at = ?10
             WHERE run = ?1 AND position = ?2",
            params![
                agent.run,
                agent.position,
                count(tokens.map(|tokens| tokens.input)),
                count(tokens.map(|tokens| tokens.output)),
                count(tokens.map(|tokens| tokens.cache_read)),
                count(tokens.map(|tokens| tokens.cache_write)),
                count(agent.cpu_ms),
                count(agent.peak_memory),
                agent.sampled_at,
                agent.done_at
            ],
        )?;
        Ok(())
    }

    /// Records a sample of an agent taken on the day starting at `day`.
    /// The first sample of each day first keeps what the agent had used
    /// before it, so a day's spending counts an agent started the day
    /// before too.
    pub fn sample_agent(&self, agent: &AgentRow, day: i64) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_days
                 (run, position, day, input, output, cache_read, cache_write)
             SELECT run, position, ?3, COALESCE(input, 0), COALESCE(output, 0),
                 COALESCE(cache_read, 0), COALESCE(cache_write, 0)
             FROM agents WHERE run = ?1 AND position = ?2",
            params![agent.run, agent.position, day],
        )?;
        self.update_agent(agent)
    }

    /// Tokens agents used since `day` started, whenever they started, for
    /// one script or all: what the daily caps count.
    pub fn tokens_on(&self, path: Option<&str>, day: i64) -> Result<Option<Tokens>> {
        let row = self.connection.query_row(
            "SELECT SUM(COALESCE(a.input, 0) - d.input), SUM(COALESCE(a.output, 0) - d.output),
                 SUM(COALESCE(a.cache_read, 0) - d.cache_read),
                 SUM(COALESCE(a.cache_write, 0) - d.cache_write), COUNT(a.input)
             FROM agent_days d JOIN agents a ON a.run = d.run AND a.position = d.position
             WHERE d.day = ?2 AND (?1 IS NULL OR a.path = ?1)",
            params![path, day],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )?;
        let (input, output, cache_read, cache_write, measured) = row;
        if measured == 0 {
            return Ok(None);
        }
        let value = |sum: Option<i64>| sum.and_then(|sum| u64::try_from(sum).ok()).unwrap_or(0);
        Ok(Some(Tokens {
            input: value(input),
            output: value(output),
            cache_read: value(cache_read),
            cache_write: value(cache_write),
        }))
    }

    // Requests from commands to the daemon

    pub fn push_request(&self, kind: &RequestKind, now: i64) -> Result<i64> {
        let (name, path, argument) = kind.columns();
        self.connection.execute(
            "INSERT INTO requests (kind, path, argument, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![name, path, argument, now],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Takes every waiting request, oldest first. One statement claims
    /// and returns them, so a request a command adds meanwhile is either
    /// taken here or left for the next call, never claimed unread.
    pub fn take_requests(&self, now: i64) -> Result<Vec<Request>> {
        let mut statement = self.connection.prepare(
            "UPDATE requests SET taken_at = ?1 WHERE taken_at IS NULL
             RETURNING id, kind, path, argument, created_at",
        )?;
        let mut requests = statement
            .query_map(params![now], Request::read)?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        requests.sort_by_key(|request| request.id);
        self.connection.execute(
            "DELETE FROM requests WHERE taken_at IS NOT NULL AND taken_at < ?1",
            params![now - 86_400],
        )?;
        Ok(requests)
    }

    // The daemon itself

    pub fn beat(&self, pid: u32, version: &str, started_at: i64, now: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO daemon (id, pid, version, started_at, beat_at) VALUES (1, ?1, ?2, ?3, ?4)
             ON CONFLICT (id) DO UPDATE SET pid = ?1, version = ?2, started_at = ?3, beat_at = ?4",
            params![pid, version, started_at, now],
        )?;
        Ok(())
    }

    /// The daemon's last heartbeat.
    pub fn daemon(&self) -> Result<Option<DaemonRow>> {
        Ok(self
            .connection
            .query_row(
                "SELECT pid, version, started_at, beat_at, paused_all FROM daemon WHERE id = 1",
                [],
                |row| {
                    Ok(DaemonRow {
                        pid: row.get(0)?,
                        version: row.get(1)?,
                        started_at: row.get(2)?,
                        beat_at: row.get(3)?,
                        paused_all: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn set_paused_all(&self, paused: bool) -> Result<()> {
        self.connection.execute(
            "INSERT INTO daemon (id, paused_all) VALUES (1, ?1)
             ON CONFLICT (id) DO UPDATE SET paused_all = ?1",
            params![paused],
        )?;
        Ok(())
    }

    pub fn paused_all(&self) -> Result<bool> {
        Ok(self.daemon()?.is_some_and(|daemon| daemon.paused_all))
    }

    /// Clears the pid when the daemon exits cleanly.
    pub fn clear_beat(&self) -> Result<()> {
        self.connection.execute(
            "UPDATE daemon SET pid = NULL, beat_at = NULL WHERE id = 1",
            [],
        )?;
        Ok(())
    }
}

fn index_value(index: usize) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX)
}

/// Reads an optional count column as `u64`.
pub(crate) fn count(row: &Row<'_>, column: usize) -> rusqlite::Result<Option<u64>> {
    Ok(row
        .get::<_, Option<i64>>(column)?
        .and_then(|value| u64::try_from(value).ok()))
}

/// For callers that need a reason's stored name, as in logs.
pub fn reason_label(reason: Reason) -> &'static str {
    rows::reason_name(reason)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
