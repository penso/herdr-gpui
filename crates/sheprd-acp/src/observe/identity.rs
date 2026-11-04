//! Resolve only the exact session advertised by the daemon. In particular a
//! Codex process can hold several child-agent rollouts: newest-file guessing
//! would silently show the wrong conversation.

use super::{Error, Result};
use crate::{
    cli::ObserveOptions,
    mirror::transcript::{Source, TranscriptAgent, codex_session},
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const HEADER: u64 = 512 * 1024;
const MAX_FDS: usize = 2048;
const MAX_PROJECTS: usize = 512;
const MAX_DAY_ENTRIES: usize = 4096;
/// Bytes of `/proc/PID/stat` read; the start time is its 22nd field.
const MAX_STAT: u64 = 8192;

pub(super) struct Identity {
    pub(super) path: PathBuf,
    pub(super) cwd: Option<String>,
    process: PathBuf,
    started: u64,
    file: File,
    agent: TranscriptAgent,
    session: String,
}

impl Identity {
    pub(super) fn open(options: &ObserveOptions) -> Result<Self> {
        if !cfg!(target_os = "linux") {
            return Err(Error::Unsupported);
        }
        Self::at(options, Path::new("/proc"), std::env::home_dir().as_deref())
    }

    pub(super) fn at(
        options: &ObserveOptions,
        proc_root: &Path,
        home: Option<&Path>,
    ) -> Result<Self> {
        let process = proc_root.join(options.pid.to_string());
        if start_ticks(&process)? != options.started {
            return Err(Error::ProcessChanged);
        }
        let cwd = fs::read_link(process.join("cwd")).ok();
        // A path the process opened twice, or that a search finds again, is
        // still one candidate.
        let mut candidates = BTreeSet::new();
        if let Ok(entries) = fs::read_dir(process.join("fd")) {
            for entry in entries.take(MAX_FDS).flatten() {
                let Ok(path) = fs::read_link(entry.path()) else {
                    continue;
                };
                if candidate_name(&path, options) {
                    candidates.insert(path);
                }
            }
        }
        // Claude opens its log only while writing. Search exact UUID names
        // within its private projects directory; never recurse over histories.
        if options.agent == TranscriptAgent::Claude
            && let Some(base) = config_directory(&process, home, TranscriptAgent::Claude)?
            && let Ok(projects) = fs::read_dir(base.join("projects"))
        {
            for project in projects.take(MAX_PROJECTS).flatten() {
                if !project.file_type()?.is_dir() {
                    continue;
                }
                let path = project.path().join(format!("{}.jsonl", options.session));
                if path.is_file() {
                    candidates.insert(path);
                }
            }
        }
        // A daemon-backed Codex TUI has no rollout descriptor of its own.
        // Its UUIDv7 bounds the lookup to three date directories; the exact
        // filename and header still have to identify the advertised thread.
        if options.agent == TranscriptAgent::Codex && candidates.is_empty() {
            candidates.extend(codex_rollouts(&process, home, options)?);
        }
        let matches: Vec<_> = candidates
            .into_iter()
            .filter(|path| matches_session(path, options).unwrap_or(false))
            .collect();
        let [path] = <[PathBuf; 1]>::try_from(matches).map_err(|_| Error::Ambiguous)?;
        let identity = Self {
            file: File::open(&path)?,
            agent: options.agent,
            session: options.session.clone(),
            path,
            cwd: cwd.map(|path| path.to_string_lossy().into_owned()),
            process,
            started: options.started,
        };
        identity.check()?;
        Ok(identity)
    }

    pub(super) fn check(&self) -> Result<()> {
        if !self.process_matches()? {
            return Err(Error::Exited);
        }
        self.check_file()
    }

    /// The pinned file is still the one at the path and still names the
    /// original session; [`Self::check`] without the process.
    pub(super) fn check_file(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let pinned = self.file.metadata()?;
            let current = fs::metadata(&self.path)?;
            if (pinned.dev(), pinned.ino()) != (current.dev(), current.ino()) {
                return Err(Error::Rotated);
            }
        }
        if !matches_file(self.file.try_clone()?, self.agent, &self.session)? {
            return Err(Error::SessionChanged);
        }
        Ok(())
    }

    pub(super) fn process_matches(&self) -> Result<bool> {
        match start_ticks(&self.process) {
            Ok(ticks) => Ok(ticks == self.started),
            Err(Error::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(super) fn source(&self) -> io::Result<Source> {
        Ok(Source::pinned(
            self.path.clone(),
            self.agent,
            self.file.try_clone()?,
        ))
    }
}

/// Read only the known config-directory override from the verified process.
/// Other environment variables never leave this bounded buffer or get logged.
fn config_directory(
    process: &Path,
    home: Option<&Path>,
    agent: TranscriptAgent,
) -> Result<Option<PathBuf>> {
    const MAX_ENVIRONMENT: u64 = 128 * 1024;
    let (variable, default): (&[u8], &str) = match agent {
        TranscriptAgent::Claude => (b"CLAUDE_CONFIG_DIR=", ".claude"),
        TranscriptAgent::Codex => (b"CODEX_HOME=", ".codex"),
    };
    let mut bytes = Vec::new();
    match File::open(process.join("environ")) {
        Ok(file) => {
            file.take(MAX_ENVIRONMENT + 1).read_to_end(&mut bytes)?;
            if bytes.len() > MAX_ENVIRONMENT as usize {
                return Err(Error::EnvironmentTooLarge);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let custom = bytes
        .split(|byte| *byte == 0)
        .find_map(|entry| entry.strip_prefix(variable))
        .filter(|value| !value.is_empty());
    let Some(custom) = custom else {
        return Ok(home.map(|home| home.join(default)));
    };
    let custom = std::str::from_utf8(custom).map_err(|_| Error::ConfigDirNotUtf8)?;
    let path = PathBuf::from(custom);
    let path = if path.is_absolute() {
        path
    } else {
        fs::read_link(process.join("cwd"))?.join(path)
    };
    Ok(Some(path))
}

fn codex_rollouts(
    process: &Path,
    home: Option<&Path>,
    options: &ObserveOptions,
) -> Result<Vec<PathBuf>> {
    let uuid = uuid::Uuid::parse_str(&options.session)?;
    if uuid.get_version() != Some(uuid::Version::SortRand) {
        return Err(Error::NotUuidV7);
    }
    let (seconds, nanos) = uuid.get_timestamp().ok_or(Error::NoTimestamp)?.to_unix();
    let date = i64::try_from(seconds)
        .ok()
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, nanos))
        .ok_or(Error::TimestampRange)?
        .date_naive();
    let Some(base) = config_directory(process, home, TranscriptAgent::Codex)? else {
        return Ok(Vec::new());
    };
    let mut candidates = Vec::new();
    // Rollout filenames may use the host's local date around UTC midnight.
    for day in [date.pred_opt(), Some(date), date.succ_opt()]
        .into_iter()
        .flatten()
    {
        let directory = base
            .join("sessions")
            .join(day.format("%Y/%m/%d").to_string());
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for (index, entry) in entries.enumerate() {
            if index >= MAX_DAY_ENTRIES {
                return Err(Error::DayTooLarge);
            }
            let entry = entry?;
            let path = entry.path();
            if candidate_name(&path, options) && entry.file_type()?.is_file() {
                candidates.push(path);
            }
        }
    }
    Ok(candidates)
}

fn start_ticks(process: &Path) -> Result<u64> {
    let mut stat = String::new();
    File::open(process.join("stat"))?
        .take(MAX_STAT)
        .read_to_string(&mut stat)?;
    // comm may contain spaces and parentheses; fields resume after its last ')'.
    let rest = stat.rsplit_once(')').ok_or(Error::InvalidStat)?.1;
    rest.split_whitespace()
        .nth(19)
        .ok_or(Error::MissingStartTime)?
        .parse()
        .map_err(Error::InvalidStartTime)
}

fn candidate_name(path: &Path, options: &ObserveOptions) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    match options.agent {
        TranscriptAgent::Codex => {
            name.starts_with("rollout-") && name.ends_with(&format!("-{}.jsonl", options.session))
        }
        TranscriptAgent::Claude => name == format!("{}.jsonl", options.session),
    }
}

fn matches_session(path: &Path, options: &ObserveOptions) -> io::Result<bool> {
    // Checked before opening, since opening a FIFO can block; `matches_file`
    // checks the opened descriptor again in case the path was replaced.
    if !fs::metadata(path)?.is_file() {
        return Ok(false);
    }
    matches_file(File::open(path)?, options.agent, &options.session)
}

fn matches_file(mut file: File, agent: TranscriptAgent, session: &str) -> io::Result<bool> {
    if !file.metadata()?.is_file() {
        return Ok(false);
    }
    file.seek(SeekFrom::Start(0))?;
    // Stop at the first session record; no reason to read an entire tail to
    // establish identity, and malformed files still have a firm byte limit.
    let reader = BufReader::new(file.take(HEADER));
    for line in reader.split(b'\n') {
        let line = line?;
        let Ok(record) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        let id = match agent {
            TranscriptAgent::Codex if record["type"] == "session_meta" => {
                codex_session(&record["payload"])
            }
            TranscriptAgent::Claude => record.get("sessionId").and_then(Value::as_str),
            TranscriptAgent::Codex => None,
        };
        if let Some(id) = id {
            return Ok(id == session);
        }
    }
    Ok(false)
}
